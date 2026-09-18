#!/usr/bin/env python3
"""
clean.py — 清理开发环境（本地用）
  1. 删除 tmp 和双系统本地仓库目录
  2. 清空数据库（PostgreSQL 删除表，SQLite 删除数据库文件）
  3. 删除双系统 Gitea 仓库

依赖：PostgreSQL 模式下需要 pip install psycopg2-binary
"""

import shutil
import urllib.request
import urllib.error
import base64
import os
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib

# ── 配置 ─────────────────────────────────────────────────────────
SCRIPT_DIR = Path(__file__).parent.resolve()
CONFIG_PATH = SCRIPT_DIR / "config" / "config.toml"

DB_HOST     = "localhost"
DB_PORT     = 5432
DB_NAME     = "wp-station"
DB_USER     = "postgres"
DB_PASS     = "123456"

GITEA_URL   = "http://127.0.0.1:3000"
GITEA_USER  = "gitea"
GITEA_PASS  = "12345678"
GITEA_TIMEOUT = 20

LOCAL_DIRS = [
    "tmp",
    "gitea/wparse__models",
    "gitea/wparse__infra",
    "gitea/wfusion__models",
    "gitea/wfusion__infra",
]

REMOTE_REPOS = [
    "wparse__models",
    "wparse__infra",
    "wfusion__models",
    "wfusion__infra",
]

DROP_TABLES = [
    'public.assist_tasks',
    'public.devices',
    'public.operation_log',
    'public.performance_results',
    'public.performance_tasks',
    'public.release_targets',
    'public.releases',
    'public.sandbox_runs',
    'public.seaql_migrations',
    'public."user"',
]

# ── 颜色输出 ─────────────────────────────────────────────────────
def info(msg):  print(f"\033[0;32m[INFO]\033[0m  {msg}")
def warn(msg):  print(f"\033[1;33m[WARN]\033[0m  {msg}")
def error(msg): print(f"\033[0;31m[ERR] \033[0m  {msg}")


def prune_empty_parent_dirs(path: Path, stop_at: Path):
    current = path
    while current != stop_at:
        if not current.exists() or not current.is_dir():
            current = current.parent
            continue

        try:
            current.rmdir()
            info(f"已删除空目录: {current}")
        except OSError:
            break

        current = current.parent


def load_config():
    if not CONFIG_PATH.exists():
        warn(f"配置文件不存在，使用默认数据库配置: {CONFIG_PATH}")
        return {}

    try:
        with CONFIG_PATH.open("rb") as f:
            return tomllib.load(f)
    except Exception as exc:
        warn(f"读取配置文件失败，使用默认数据库配置: {exc}")
        return {}


def resolve_sqlite_path(database_url: str) -> Path | None:
    raw = (database_url or "").strip()
    if not raw.startswith("sqlite:"):
        return None

    sqlite_path = raw.removeprefix("sqlite://")
    if sqlite_path == raw:
        sqlite_path = raw.removeprefix("sqlite:")

    if not sqlite_path:
        return None

    path = Path(sqlite_path)
    if path.is_absolute():
        return path
    return (SCRIPT_DIR / path).resolve()


def cleanup_sqlite_database(database_url: str):
    db_path = resolve_sqlite_path(database_url)
    if not db_path:
        error(f"无法解析 SQLite 数据库路径: {database_url}")
        return

    targets = [db_path, Path(f"{db_path}-shm"), Path(f"{db_path}-wal")]
    for target in targets:
        if target.exists():
            target.unlink()
            info(f"已删除 SQLite 文件: {target}")
        else:
            warn(f"SQLite 文件不存在，跳过: {target}")

    prune_empty_parent_dirs(db_path.parent, SCRIPT_DIR)
    if any(target.exists() for target in targets):
        warn("SQLite 数据库文件未完全清理，请检查文件占用情况")
    else:
        info("SQLite 数据库文件已清理完成")

# ─────────────────────────────────────────────────────────────────
# 1. 删除本地目录
# ─────────────────────────────────────────────────────────────────
def step_delete_dirs():
    info("=== 1/3 删除本地目录 ===")
    for name in LOCAL_DIRS:
        target = (SCRIPT_DIR / name).resolve()
        if target.exists():
            shutil.rmtree(target)
            info(f"已删除: {target}")
            prune_empty_parent_dirs(target.parent, SCRIPT_DIR)
        else:
            warn(f"目录不存在，跳过: {target}")

# ─────────────────────────────────────────────────────────────────
# 2. 清空数据库表
# ─────────────────────────────────────────────────────────────────
def step_drop_tables():
    info("=== 2/3 清空数据库表 ===")
    config = load_config()
    database = config.get("database", {})
    database_url = str(database.get("url", "")).strip()

    if database_url.startswith("sqlite:"):
        info("检测到 SQLite 数据库，改为清理数据库文件")
        cleanup_sqlite_database(database_url)
        return

    try:
        import psycopg2
    except ImportError:
        error("缺少 psycopg2，请先执行: pip install psycopg2-binary")
        return

    try:
        conn = psycopg2.connect(
            host=database.get("host", DB_HOST),
            port=database.get("port", DB_PORT),
            dbname=database.get("name", DB_NAME),
            user=database.get("username", DB_USER),
            password=database.get("password", DB_PASS),
        )
        conn.autocommit = True
        cur = conn.cursor()
        for table in DROP_TABLES:
            cur.execute(f"DROP TABLE IF EXISTS {table} CASCADE;")
            info(f"已删除表: {table}")
        cur.close()
        conn.close()
        info("数据库表已全部删除")
    except Exception as e:
        error(f"数据库操作失败: {e}")

# ─────────────────────────────────────────────────────────────────
# 3. 删除 Gitea 仓库
# ─────────────────────────────────────────────────────────────────
def delete_gitea_repo(repo: str) -> bool:
    """删除单个 Gitea 仓库；仓库不存在视为成功，临时错误自动重试。"""
    gitea_url = os.environ.get("WARP_STATION_GITEA_URL", GITEA_URL).rstrip("/")
    gitea_user = os.environ.get("WARP_STATION_GITEA_USER", GITEA_USER)
    gitea_pass = os.environ.get("WARP_STATION_GITEA_PASS", GITEA_PASS)
    url = f"{gitea_url}/api/v1/repos/{gitea_user}/{repo}"
    token = base64.b64encode(f"{gitea_user}:{gitea_pass}".encode()).decode()
    req = urllib.request.Request(
        url,
        method="DELETE",
        headers={"Authorization": f"Basic {token}"},
    )
    try:
        with urllib.request.urlopen(req, timeout=GITEA_TIMEOUT) as response:
            if 200 <= response.status < 300:
                info(f"已删除 Gitea 仓库: {gitea_user}/{repo}")
                return True
            error(f"删除仓库失败: repo={gitea_user}/{repo}, http={response.status}")
            return False
    except urllib.error.HTTPError as e:
        # 404 幂等处理：仓库已被删除时不应阻断整次清理。
        if e.code == 404:
            warn(f"仓库不存在，跳过: {gitea_user}/{repo}")
            return True

        body = e.read().decode("utf-8", errors="replace").strip()
        detail = f"，响应={body[:300]}" if body else ""
        error(f"删除仓库失败: repo={gitea_user}/{repo}, http={e.code}{detail}")
        return False
    except urllib.error.URLError as e:
        error(f"无法连接 Gitea: repo={gitea_user}/{repo}, error={e.reason}")
        return False
    except OSError as e:
        error(f"Gitea 连接异常: repo={gitea_user}/{repo}, error={e}")
        return False

    return False

def step_delete_gitea_repos():
    info("=== 3/3 删除 Gitea 仓库 ===")
    failed = []
    for repo in REMOTE_REPOS:
        if not delete_gitea_repo(repo):
            failed.append(repo)
    if failed:
        error(f"Gitea 仓库清理失败: {', '.join(failed)}")
        return False
    return True

# ─────────────────────────────────────────────────────────────────
if __name__ == "__main__":
    step_delete_dirs()
    step_drop_tables()
    if step_delete_gitea_repos():
        info("=== 清理完成 ===")
    else:
        error("=== 清理未完成 ===")
        raise SystemExit(1)
