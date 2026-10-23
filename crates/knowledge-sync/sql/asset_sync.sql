-- 资产同步结构，由应用按版本管理并幂等初始化。
CREATE EXTENSION IF NOT EXISTS dblink;
CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE SCHEMA IF NOT EXISTS asset_lookup;

CREATE OR REPLACE FUNCTION asset_lookup.inet_to_num(p_ip inet)
RETURNS numeric
LANGUAGE plpgsql
IMMUTABLE
STRICT
PARALLEL SAFE
AS $function$
DECLARE
    v_bytes  bytea;
    v_result numeric := 0;
    i        integer;
BEGIN
    IF family(p_ip) = 4 THEN
        RETURN split_part(host(p_ip), '.', 1)::numeric * 16777216
             + split_part(host(p_ip), '.', 2)::numeric * 65536
             + split_part(host(p_ip), '.', 3)::numeric * 256
             + split_part(host(p_ip), '.', 4)::numeric;
    END IF;

    v_bytes := inet_send(p_ip);

    FOR i IN 2..17 LOOP
        v_result := v_result * 256 + get_byte(v_bytes, i);
    END LOOP;

    RETURN v_result
         + 340282366920938463463374607431768211456::numeric;
END;
$function$;

CREATE TABLE IF NOT EXISTS asset_lookup.asset_subject_exclude_biz_sync (
    source_key       text PRIMARY KEY,
    network          text,
    asset_id         text NOT NULL,
    tenant_id        text,
    asset_type       text,
    asset_type_name  text,
    asset_name       text,
    ip_type          text,
    start_ip_num     numeric(40, 0),
    end_ip_num       numeric(40, 0),
    ip               text,
    company_id       text,
    company_name     text,
    system_id        text,
    system_name      text,
    synced_at        timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT asset_subject_exclude_biz_sync_range_ck
        CHECK (
            (start_ip_num IS NULL AND end_ip_num IS NULL)
            OR start_ip_num <= end_ip_num
        )
);

-- GiST 同时处理 IP 区间两侧条件。不要单独增加 start_ip_num B-tree，
-- 通用预备计划可能优先选择该索引，导致未命中查询扫描大量 IPv4 记录。
CREATE INDEX IF NOT EXISTS asset_subject_exclude_biz_sync_num_gist_idx
ON asset_lookup.asset_subject_exclude_biz_sync
USING gist (start_ip_num, end_ip_num)
INCLUDE (
    asset_id,
    asset_type,
    asset_name,
    system_id,
    system_name,
    company_name
);

CREATE TABLE IF NOT EXISTS asset_lookup.sync_run_summary (
    run_id bigserial PRIMARY KEY,
    started_at timestamptz NOT NULL,
    finished_at timestamptz NOT NULL,
    asset_count bigint NOT NULL,
    network_count bigint NOT NULL,
    written_count bigint NOT NULL
);
ALTER TABLE asset_lookup.sync_run_summary ADD COLUMN IF NOT EXISTS terminal_count bigint;
ALTER TABLE asset_lookup.sync_run_summary ADD COLUMN IF NOT EXISTS cron_run_id bigint;
ALTER TABLE asset_lookup.sync_run_summary ADD COLUMN IF NOT EXISTS ipv4_count bigint;
ALTER TABLE asset_lookup.sync_run_summary ADD COLUMN IF NOT EXISTS ipv6_count bigint;
CREATE INDEX IF NOT EXISTS sync_run_summary_cron_run_id_idx
ON asset_lookup.sync_run_summary (cron_run_id);

CREATE OR REPLACE PROCEDURE asset_lookup.refresh_asset_subject_lookup()
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, asset_lookup
AS $procedure$
DECLARE
    v_locked    boolean;
    v_new_count bigint;
    v_started_at timestamptz := clock_timestamp();
    v_asset_count bigint;
    v_terminal_count bigint;
    v_network_count bigint;
    v_ipv4_count bigint;
    v_ipv6_count bigint;
    v_cron_run_id bigint;
BEGIN
    v_locked := pg_try_advisory_lock(8412, 20260914);

    IF NOT v_locked THEN
        RAISE NOTICE 'asset sync skipped: another run is active';
        RETURN;
    END IF;

    BEGIN
        CREATE TEMP TABLE asset_subject_exclude_biz_tmp
        (LIKE asset_lookup.asset_subject_exclude_biz_sync
            INCLUDING DEFAULTS
            INCLUDING CONSTRAINTS)
        ON COMMIT DROP;

        PERFORM public.dblink_connect(
            'dysec_asset_conn',
            'dysec_asset_source'
        );

        -- 网段只取 data.cidr；HOST 和 TERMINAL 的 ip/public_ip 各生成独立查询行。
        INSERT INTO pg_temp.asset_subject_exclude_biz_tmp (
            source_key, network, asset_id, tenant_id, asset_type, asset_type_name,
            asset_name, ip_type, start_ip_num, end_ip_num, ip, company_id,
            company_name, system_id, system_name
        )
        SELECT
            CASE WHEN address.kind = 'public_ip'
                 THEN src.asset_id || ':public_ip' ELSE src.asset_id END,
            CASE WHEN address.kind = 'network' THEN address.address_text ELSE NULL END,
            src.asset_id,
            COALESCE(src.tenant_id, '0'),
            src.manage_asset_type,
            COALESCE(src.manage_asset_type_name, src.asset_type_name),
            src.asset_name,
            CASE WHEN family(bounds.start_addr) = 6 THEN 'ipv6' ELSE 'ipv4' END,
            asset_lookup.inet_to_num(bounds.start_addr),
            asset_lookup.inet_to_num(bounds.end_addr),
            CASE WHEN address.kind IN ('ip', 'public_ip') THEN address.address_text ELSE NULL END,
            src.company_id,
            src.company_name,
            src.system_id,
            src.system_name
        FROM public.dblink(
            'dysec_asset_conn',
            $remote$
            SELECT
                asset_id::text,
                manage_asset_type::text,
                manage_asset_type_name::text,
                asset_type_name::text,
                asset_name::text,
                tenant_id::text,
                NULLIF(btrim(ip), '')::text AS source_ip,
                NULLIF(btrim(public_ip), '')::text AS public_ip,
                NULLIF(btrim(data ->> 'cidr'), '')::text AS network_text,
                NULLIF(responsible_org::text, '') AS company_id,
                COALESCE(NULLIF(responsible_org_name::text, ''),
                         NULLIF(business_org_name::text, ''),
                         NULLIF(data ->> 'responsible_org_name', ''),
                         NULLIF(data ->> 'business_org_name', ''))::text AS company_name,
                NULLIF(responsible_system::text, '') AS system_id,
                NULLIF(responsible_system_name::text, '') AS system_name
            FROM public.asset_subject
            WHERE manage_asset_type IN ('HOST', 'TERMINAL', 'NET_WORK')
            $remote$
        ) AS src (
            asset_id text, manage_asset_type text, manage_asset_type_name text,
            asset_type_name text, asset_name text, tenant_id text,
            source_ip text, public_ip text, network_text text, company_id text,
            company_name text, system_id text, system_name text
        )
        CROSS JOIN LATERAL (
            VALUES
                ('network', CASE WHEN src.manage_asset_type = 'NET_WORK' THEN src.network_text END),
                ('ip', CASE WHEN src.manage_asset_type IN ('HOST', 'TERMINAL') THEN src.source_ip END),
                ('public_ip', CASE WHEN src.manage_asset_type IN ('HOST', 'TERMINAL') THEN src.public_ip END)
        ) AS address(kind, address_text)
        CROSS JOIN LATERAL (
            SELECT
                CASE
                    WHEN address.kind IN ('ip', 'public_ip')
                        THEN host(address.address_text::inet)::inet
                    WHEN address.address_text LIKE '%-%'
                        THEN btrim(split_part(address.address_text, '-', 1))::inet
                    ELSE network(address.address_text::inet)::inet
                END AS start_addr,
                CASE
                    WHEN address.kind IN ('ip', 'public_ip')
                        THEN host(address.address_text::inet)::inet
                    WHEN address.address_text LIKE '%-%'
                        THEN btrim(split_part(address.address_text, '-', 2))::inet
                    ELSE broadcast(address.address_text::inet)
                END AS end_addr
        ) AS bounds
        WHERE address.address_text IS NOT NULL;

        PERFORM public.dblink_disconnect('dysec_asset_conn');

        SELECT count(*)
        INTO v_new_count
        FROM pg_temp.asset_subject_exclude_biz_tmp;

        IF v_new_count = 0 THEN
            RAISE EXCEPTION
                'asset sync rejected: source returned zero rows';
        END IF;

        IF EXISTS (
            SELECT 1
            FROM pg_temp.asset_subject_exclude_biz_tmp
            GROUP BY source_key
            HAVING count(*) > 1
        ) THEN
            RAISE EXCEPTION
                'asset sync rejected: duplicate source_key';
        END IF;

        IF EXISTS (
            SELECT 1
            FROM pg_temp.asset_subject_exclude_biz_tmp
            WHERE (start_ip_num IS NULL) <> (end_ip_num IS NULL)
               OR start_ip_num > end_ip_num
        ) THEN
            RAISE EXCEPTION
                'asset sync rejected: invalid numeric range';
        END IF;

        -- PostgreSQL 的 TRUNCATE 支持事务回滚，可避免全量刷新积累死元组和 GiST 膨胀；
        -- 后续 INSERT 失败时，事务回滚会恢复上一版数据。
        TRUNCATE TABLE asset_lookup.asset_subject_exclude_biz_sync;

        INSERT INTO asset_lookup.asset_subject_exclude_biz_sync (
            source_key,
            network,
            asset_id,
            tenant_id,
            asset_type,
            asset_type_name,
            asset_name,
            ip_type,
            start_ip_num,
            end_ip_num,
            ip,
            company_id,
            company_name,
            system_id,
            system_name,
            synced_at
        )
        SELECT
            source_key,
            network,
            asset_id,
            tenant_id,
            asset_type,
            asset_type_name,
            asset_name,
            ip_type,
            start_ip_num,
            end_ip_num,
            ip,
            company_id,
            company_name,
            system_id,
            system_name,
            clock_timestamp()
        FROM pg_temp.asset_subject_exclude_biz_tmp;

        ANALYZE asset_lookup.asset_subject_exclude_biz_sync;

        SELECT
            count(*) FILTER (WHERE asset_type = 'HOST'),
            count(*) FILTER (WHERE asset_type = 'TERMINAL'),
            count(*) FILTER (WHERE asset_type = 'NET_WORK'),
            count(*) FILTER (WHERE lower(ip_type) = 'ipv4'),
            count(*) FILTER (WHERE lower(ip_type) = 'ipv6')
        INTO v_asset_count, v_terminal_count, v_network_count, v_ipv4_count, v_ipv6_count
        FROM pg_temp.asset_subject_exclude_biz_tmp;

        -- pg_cron 在执行前写入 running 记录；手动调用时允许 run_id 为空。
        IF to_regclass('cron.job_run_details') IS NOT NULL THEN
            SELECT runid INTO v_cron_run_id
            FROM cron.job_run_details
            WHERE job_pid = pg_backend_pid() AND status = 'running'
            ORDER BY runid DESC LIMIT 1;
        END IF;

        INSERT INTO asset_lookup.sync_run_summary (
            started_at, finished_at, asset_count, terminal_count, network_count, written_count,
            ipv4_count, ipv6_count, cron_run_id
        ) VALUES (
            v_started_at, clock_timestamp(), v_asset_count, v_terminal_count, v_network_count, v_new_count,
            v_ipv4_count, v_ipv6_count, v_cron_run_id
        );

        PERFORM pg_advisory_unlock(8412, 20260914);
    EXCEPTION WHEN OTHERS THEN
        BEGIN
            PERFORM public.dblink_disconnect('dysec_asset_conn');
        EXCEPTION WHEN OTHERS THEN
            NULL;
        END;

        PERFORM pg_advisory_unlock(8412, 20260914);
        RAISE;
    END;
END;
$procedure$;

CREATE OR REPLACE VIEW public.v_asset_subject_exclude_biz AS
SELECT
    network,
    asset_id,
    tenant_id,
    asset_type,
    asset_type_name,
    asset_name,
    ip_type,
    start_ip_num,
    end_ip_num,
    ip,
    company_id,
    company_name,
    system_id,
    system_name,
    CASE asset_type
        WHEN 'HOST' THEN 3
        WHEN 'TERMINAL' THEN 2
        WHEN 'NET_WORK' THEN 1
        ELSE 0
    END AS asset_priority
FROM asset_lookup.asset_subject_exclude_biz_sync;
