import httpRequest from './request';

/** 获取知识库同步任务状态。 */
export async function fetchKnowledgeSyncStatus() {
  const response = await httpRequest.get('/knowledge-sync/status');
  const payload = response?.asset ? response : response?.data || response || {};
  const asset = payload?.asset || {};
  const geo = payload?.geo || {};
  const intel = payload?.intel || {};
  const latestGeoRun = geo.latest_run || null;

  return {
    asset: {
      configured: Boolean(asset.configured),
      initialized: Boolean(asset.initialized),
      database: asset.database || '',
      taskName: asset.task_name || '',
      taskActive: typeof asset.task_active === 'boolean' ? asset.task_active : null,
      taskSchedule: asset.task_schedule || '',
      scheduleMatches:
        typeof asset.schedule_matches === 'boolean' ? asset.schedule_matches : null,
      latestRunStatus: asset.latest_run_status || '',
      latestRunStartedAt: asset.latest_run_started_at || '',
      latestRunFinishedAt: asset.latest_run_finished_at || '',
      latestRunMessage: asset.latest_run_message || '',
      latestSuccessAt: asset.latest_success_at || '',
      currentRowCount:
        typeof asset.current_row_count === 'number' ? asset.current_row_count : null,
      currentIpv4Count: asset.current_ipv4_count ?? null,
      currentIpv6Count: asset.current_ipv6_count ?? null,
      latestWrittenCount:
        typeof asset.latest_written_count === 'number' ? asset.latest_written_count : null,
      latestAssetCount:
        typeof asset.latest_asset_count === 'number' ? asset.latest_asset_count : null,
      latestTerminalCount:
        typeof asset.latest_terminal_count === 'number' ? asset.latest_terminal_count : null,
      latestNetworkCount:
        typeof asset.latest_network_count === 'number' ? asset.latest_network_count : null,
      latestIpv4Count: asset.latest_ipv4_count ?? null,
      latestIpv6Count: asset.latest_ipv6_count ?? null,
      latestDataAt: asset.latest_data_at || '',
      error: asset.error || '',
    },
    geo: {
      configured: Boolean(geo.configured),
      sampleLimit: geo.sample_limit ?? null,
      tableExists: Boolean(geo.table_exists),
      indexReady: Boolean(geo.index_ready),
      currentRowCount: geo.current_row_count ?? null,
      sourceRowCount: geo.source_row_count ?? null,
      ipv6SourceRowCount: geo.ipv6_source_row_count ?? null,
      latestDataAt: geo.latest_data_at || '',
      fileName: geo.file_name || '',
      fileHash: geo.file_hash || '',
      databaseType: geo.database_type || '',
      buildEpoch: geo.build_epoch ?? null,
      latestRun: latestGeoRun ? mapGeoRun(latestGeoRun) : null,
      error: geo.error || '',
    },
    intel: {
      configured: Boolean(intel.configured),
      intervalSeconds: intel.interval_seconds ?? null,
      ipCount: intel.ip_count ?? null,
      domainCount: intel.domain_count ?? null,
      latestRun: intel.latest_run ? mapIntelRun(intel.latest_run) : null,
      latestSuccessRun: intel.latest_success_run ? mapIntelRun(intel.latest_success_run) : null,
      error: intel.error || '',
    },
  };
}

function mapGeoRun(run) {
  return {
    id: run.id,
    status: run.status,
    stage: run.stage,
    fileName: run.file_name,
    fileHash: run.file_hash || '',
    rowCount: run.row_count ?? null,
    sourceRowCount: run.source_row_count ?? null,
    ipv6SourceRowCount: run.ipv6_source_row_count ?? null,
    rowLimit: run.row_limit ?? null,
    progressRows: run.progress_rows ?? 0,
    progressPercent: run.progress_percent ?? null,
    error: run.error || '',
    startedAt: run.started_at || '',
    finishedAt: run.finished_at || '',
  };
}

function mapIntelRun(run) {
  return {
    id: run.id,
    status: run.status,
    stage: run.stage,
    readCount: run.read_count ?? 0,
    invalidCount: run.invalid_count ?? 0,
    expiredCount: run.expired_count ?? 0,
    ipCount: run.ip_count ?? 0,
    domainCount: run.domain_count ?? 0,
    error: run.error || '',
    startedAt: run.started_at || '',
    finishedAt: run.finished_at || '',
  };
}

/** 分页读取资产任务历史，记录直接来自 pg_cron。 */
export async function fetchAssetRuns(page = 1, pageSize = 20) {
  const response = await httpRequest.get('/knowledge-sync/asset-runs', {
    params: { limit: pageSize, offset: (page - 1) * pageSize },
  });
  const payload = response?.items ? response : response?.data || { items: [], total: 0 };
  return {
    total: payload.total || 0,
    items: (payload.items || []).map((run) => ({
      id: run.run_id,
      status: run.status,
      startedAt: run.started_at || '',
      finishedAt: run.finished_at || '',
      message: run.message || '',
      writtenCount: run.written_count ?? null,
      assetCount: run.asset_count ?? null,
      terminalCount: run.terminal_count ?? null,
      networkCount: run.network_count ?? null,
      totalAssetCount: run.asset_count == null || run.terminal_count == null || run.network_count == null
        ? null : run.asset_count + run.terminal_count + run.network_count,
      ipv4Count: run.ipv4_count ?? null,
      ipv6Count: run.ipv6_count ?? null,
    })),
  };
}

/** 分页读取目标库中的 Geo 导入记录。 */
export async function fetchGeoRuns(page = 1, pageSize = 20) {
  const response = await httpRequest.get('/knowledge-sync/geo-runs', {
    params: { limit: pageSize, offset: (page - 1) * pageSize },
  });
  const payload = response?.items ? response : response?.data || { items: [], total: 0 };
  return { total: payload.total || 0, items: (payload.items || []).map(mapGeoRun) };
}

/** 分页读取情报同步历史。 */
export async function fetchIntelRuns(page = 1, pageSize = 20) {
  const response = await httpRequest.get('/knowledge-sync/intel-runs', {
    params: { limit: pageSize, offset: (page - 1) * pageSize },
  });
  const payload = response?.items ? response : response?.data || { items: [], total: 0 };
  return { total: payload.total || 0, items: (payload.items || []).map(mapIntelRun) };
}

/** 清空指定类别的同步执行历史。 */
export async function clearSyncRuns(kind) {
  if (!['asset', 'intel'].includes(kind)) {
    throw new Error('无效的同步历史类型');
  }
  const response = await httpRequest.delete(`/knowledge-sync/runs/${kind}/history`);
  return response?.deleted ?? response?.data?.deleted ?? 0;
}

/** 列出固定目录中的 MMDB 文件。 */
export async function fetchMmdbFiles() {
  const response = await httpRequest.get('/knowledge-sync/mmdb-files');
  const files = Array.isArray(response) ? response : response?.data || [];
  return files.map((file) => ({
    name: file.name,
    size: file.size,
    modifiedAt: file.modified_at,
  }));
}

/** 启动 MMDB 导入任务。 */
export async function startGeoImport(fileName) {
  const response = await httpRequest.post('/knowledge-sync/geo-imports', { file_name: fileName });
  return mapGeoRun(response?.data || response);
}

/** 读取 MMDB 导入任务进度。 */
export async function fetchGeoRun(id) {
  const response = await httpRequest.get(`/knowledge-sync/runs/${id}`);
  return mapGeoRun(response?.data || response);
}
