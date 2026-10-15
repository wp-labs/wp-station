#!/usr/bin/env python3
"""清理知识库同步的本地测试产物；不删除资产、Doris、MMDB 源数据。"""

import argparse
import os
import socket
import sqlite3
import ssl
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

ROOT = Path(__file__).resolve().parent
DEFAULT_PG_URL = "postgres://postgres:123456@127.0.0.1:5432/postgres"
DEFAULT_REDIS_URL = "redis://:123456@127.0.0.1:6379/0"
ASSET_JOB_NAME = "asset_subject_lookup_refresh"
ASSET_COMMAND = "CALL asset_lookup.refresh_asset_subject_lookup();"
REDIS_KEYS = {"threat_intelligence:ip", "threat_intelligence:domain", "{intel}:ip", "{intel}:domain"}
REDIS_PREFIXES = (
    "threat_intelligence:ip:staging:",
    "threat_intelligence:domain:staging:",
    "threat_intelligence:ip:backup:",
    "threat_intelligence:domain:backup:",
)


def station_running() -> bool:
    """运行中的 Station 会重新写入 Redis，必须先停止。"""
    result = subprocess.run(
        ["ps", "-Ao", "pid=,comm="], capture_output=True, text=True, check=True
    )
    for line in result.stdout.splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) == 2 and parts[0].isdigit() and int(parts[0]) != os.getpid():
            if Path(parts[1]).name == "wp-station":
                return True
    return False


def station_sqlite_path() -> Path:
    """从 Station 现有配置读取 SQLite 路径，不新增连接配置。"""
    with (ROOT / "config" / "config.toml").open("rb") as stream:
        config = tomllib.load(stream)
    url = str(config.get("database", {}).get("url", ""))
    if not url.startswith("sqlite:"):
        raise ValueError("Station 配置未使用 SQLite，请用 --sqlite-db 指定运行记录库")
    raw = url.removeprefix("sqlite://") if url.startswith("sqlite://") else url.removeprefix("sqlite:")
    raw = raw.split("?", 1)[0]
    if not raw:
        raise ValueError("Station SQLite 路径为空")
    path = Path(raw)
    return path if path.is_absolute() else ROOT / path


class RedisConnection:
    """仅供清理脚本使用的最小 RESP 客户端，无额外 Python 依赖。"""

    def __init__(self, url: str):
        parsed = urlsplit(url)
        if parsed.scheme not in ("redis", "rediss") or not parsed.hostname:
            raise ValueError("Redis URL 必须是 redis:// 或 rediss://")
        sock = socket.create_connection((parsed.hostname, parsed.port or 6379), timeout=10)
        if parsed.scheme == "rediss":
            sock = ssl.create_default_context().wrap_socket(sock, server_hostname=parsed.hostname)
        self.sock = sock
        self.stream = sock.makefile("rb")
        if parsed.password is not None:
            password = unquote(parsed.password)
            if parsed.username:
                self.execute("AUTH", unquote(parsed.username), password)
            else:
                self.execute("AUTH", password)
        database = parsed.path.lstrip("/") or "0"
        if not database.isdecimal():
            raise ValueError("Redis 数据库编号无效")
        if database != "0":
            self.execute("SELECT", database)

    def close(self):
        self.stream.close()
        self.sock.close()

    def execute(self, *parts):
        values = [str(part).encode("utf-8") for part in parts]
        payload = [f"*{len(values)}\r\n".encode()]
        for value in values:
            payload.extend((f"${len(value)}\r\n".encode(), value, b"\r\n"))
        self.sock.sendall(b"".join(payload))
        return self._read()

    def _read(self):
        line = self.stream.readline()
        if not line.endswith(b"\r\n"):
            raise ConnectionError("Redis 响应不完整")
        kind, value = line[:1], line[1:-2]
        if kind == b"+":
            return value.decode()
        if kind == b"-":
            raise RuntimeError("Redis 命令失败: " + value.decode(errors="replace"))
        if kind == b":":
            return int(value)
        if kind == b"$":
            size = int(value)
            if size < 0:
                return None
            data = self.stream.read(size + 2)
            if len(data) != size + 2 or not data.endswith(b"\r\n"):
                raise ConnectionError("Redis bulk 响应不完整")
            return data[:-2]
        if kind == b"*":
            count = int(value)
            return [self._read() for _ in range(count)] if count >= 0 else None
        raise RuntimeError("Redis 响应类型无效")

    def matching_keys(self) -> list[str]:
        keys = set()
        cursor = 0
        while True:
            cursor_raw, batch = self.execute("SCAN", cursor, "MATCH", "*intel*", "COUNT", 1000)
            cursor = int(cursor_raw)
            for raw in batch:
                key = raw.decode("utf-8")
                if key in REDIS_KEYS or key.startswith(REDIS_PREFIXES):
                    keys.add(key)
            if cursor == 0:
                return sorted(keys)


def inspect_postgres(connection) -> tuple[str, list[int], int]:
    with connection.cursor() as cursor:
        cursor.execute("SELECT current_database()")
        database = cursor.fetchone()[0]
        cursor.execute("SELECT to_regclass('cron.job') IS NOT NULL")
        if cursor.fetchone()[0]:
            cursor.execute(
                "SELECT jobid FROM cron.job WHERE database=current_database() "
                "AND (jobname=%s OR command=%s) ORDER BY jobid",
                (ASSET_JOB_NAME, ASSET_COMMAND),
            )
            jobs = [row[0] for row in cursor.fetchall()]
        else:
            jobs = []
        cursor.execute("SELECT to_regclass('cron.job_run_details') IS NOT NULL")
        if cursor.fetchone()[0]:
            cursor.execute(
                "SELECT count(*) FROM cron.job_run_details "
                "WHERE database=current_database() AND command=%s", (ASSET_COMMAND,)
            )
            runs = cursor.fetchone()[0]
        else:
            runs = 0
    return database, jobs, runs


def clean_postgres(connection, jobs: list[int]) -> None:
    from psycopg2 import sql

    with connection:
        with connection.cursor() as cursor:
            for job_id in jobs:
                cursor.execute("SELECT cron.unschedule(%s)", (job_id,))
            cursor.execute("SELECT to_regclass('cron.job_run_details') IS NOT NULL")
            if cursor.fetchone()[0]:
                cursor.execute(
                    "DELETE FROM cron.job_run_details WHERE database=current_database() "
                    "AND (command=%s OR jobid=ANY(%s))", (ASSET_COMMAND, jobs)
                )
            cursor.execute(
                "SELECT tablename FROM pg_tables WHERE schemaname='public' "
                "AND tablename ~ '^ip_geo_(stage|old)_[0-9]+$'"
            )
            for (name,) in cursor.fetchall():
                cursor.execute(sql.SQL("DROP TABLE IF EXISTS public.{}").format(sql.Identifier(name)))
            # 不使用 CASCADE，遇到其他系统依赖时中止并回滚。
            for statement in (
                "DROP VIEW IF EXISTS public.v_asset_subject_exclude_biz",
                "DROP PROCEDURE IF EXISTS asset_lookup.refresh_asset_subject_lookup()",
                "DROP TABLE IF EXISTS asset_lookup.asset_subject_exclude_biz_sync",
                "DROP TABLE IF EXISTS asset_lookup.sync_run_summary",
                "DROP TABLE IF EXISTS asset_lookup.station_schema_version",
                "DROP FUNCTION IF EXISTS asset_lookup.inet_to_num(inet)",
                "DROP SCHEMA IF EXISTS asset_lookup",
                "DROP TABLE IF EXISTS public.ip_geo",
                "DROP TABLE IF EXISTS knowledge_sync.geo_runs",
                "DROP TABLE IF EXISTS knowledge_sync.intel_runs",
                "DROP SCHEMA IF EXISTS knowledge_sync",
            ):
                cursor.execute(statement)


def inspect_sqlite(path: Path) -> tuple[sqlite3.Connection | None, dict[str, int]]:
    if not path.is_file():
        return None, {"geo_sync_runs": 0, "intel_sync_runs": 0}
    connection = sqlite3.connect(f"file:{path}?mode=rw", uri=True, timeout=10)
    names = {row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    counts = {}
    for name in ("geo_sync_runs", "intel_sync_runs"):
        counts[name] = connection.execute(f"SELECT count(*) FROM {name}").fetchone()[0] if name in names else 0
    return connection, counts


def clean_sqlite(connection: sqlite3.Connection):
    names = {row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    with connection:
        for name in ("geo_sync_runs", "intel_sync_runs"):
            if name in names:
                connection.execute(f"DELETE FROM {name}")
                if "sqlite_sequence" in names:
                    connection.execute("DELETE FROM sqlite_sequence WHERE name=?", (name,))
    connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--pg-url",
        default=os.getenv("WP_KSYNC_TARGET_URL")
        or os.getenv("WP_KSYNC_ASSET_TARGET_URL")
        or os.getenv("WP_KSYNC_GEO_TARGET_URL")
        or DEFAULT_PG_URL,
        help="目标 PostgreSQL URL；优先读取部署环境变量，未配置时使用本地 postgres",
    )
    parser.add_argument(
        "--redis-url",
        default=os.getenv("WP_KSYNC_REDIS_URL") or DEFAULT_REDIS_URL,
        help="目标 Redis URL；优先读取部署环境变量，未配置时使用本地 Redis 0 号库",
    )
    parser.add_argument("--sqlite-db", type=Path, help="Station SQLite 文件；默认读取 config/config.toml")
    parser.add_argument("--dry-run", action="store_true", help="只预览清理范围，不删除数据")
    args = parser.parse_args()
    if not args.dry_run and station_running():
        parser.error("检测到 wp-station 正在运行，请先停止服务后再清理")

    try:
        import psycopg2
    except ImportError as error:
        raise SystemExit("请先安装 psycopg2-binary: pip install psycopg2-binary") from error
    sqlite_path = args.sqlite_db or station_sqlite_path()
    pg = psycopg2.connect(args.pg_url, connect_timeout=10)
    redis = None
    station_db = None
    try:
        database, jobs, runs = inspect_postgres(pg)
        station_db, counts = inspect_sqlite(sqlite_path)
        redis = RedisConnection(args.redis_url)
        redis.execute("PING")
        keys = redis.matching_keys()
        pg_host = urlsplit(args.pg_url).hostname
        redis_target = urlsplit(args.redis_url)
        print(f"目标 PostgreSQL: {pg_host}/{database}；资产 cron: {len(jobs)}；资产历史: {runs}")
        sqlite_state = "文件不存在，跳过" if station_db is None else f"Geo 历史: {counts['geo_sync_runs']}；情报历史: {counts['intel_sync_runs']}"
        print(f"Station SQLite: {sqlite_path}；{sqlite_state}")
        print(f"目标 Redis: {redis_target.hostname}/{redis_target.path.lstrip('/') or '0'}；情报键: {len(keys)}")
        if args.dry_run:
            print("仅预览；不删除数据")
            return 0
        clean_postgres(pg, jobs)
        print("PostgreSQL 资产任务、资产对象、ip_geo 和旧同步对象已清理")
        for index in range(0, len(keys), 100):
            redis.execute("UNLINK", *keys[index:index + 100])
        print(f"Redis 情报键已清理: {len(keys)}")
        if station_db is not None:
            clean_sqlite(station_db)
            print("Station SQLite 同步历史已清理")
        print("源数据与其他 Station 数据保留")
        return 0
    finally:
        if station_db is not None:
            station_db.close()
        if redis is not None:
            redis.close()
        pg.close()


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print(f"清理失败: {error}", file=sys.stderr)
        sys.exit(1)
