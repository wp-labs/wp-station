/**
 * 系统发布服务模块
 * 提供发布列表查询、发布详情查询、校验和发布功能
 * 已接入后端 /api/releases 相关接口
 */

import httpRequest from './request';
import { resolveSystem } from './system';

/**
 * 获取发布列表
 * @param {Object} options - 查询选项
 * @param {string} [options.version] - 版本号
 * @param {string} [options.pipeline] - 流水线关键词
 * @param {string} [options.createdBy] - 发布人
 * @param {string} [options.status] - 状态（WAIT/INIT/PASS/FAIL）
 * @param {number} [options.page] - 页码（默认 1）
 * @param {number} [options.pageSize] - 每页条数（默认 10）
 * @returns {Promise<Object>} 分页结果
 */
export async function fetchReleases(options = {}) {
  const { version, pipeline, createdBy, status, page = 1, pageSize = 10, system } = options;

  const response = await httpRequest.get('/releases', {
    params: {
      system: resolveSystem(system),
      version: version || undefined,
      pipeline: pipeline || undefined,
      created_by: createdBy || undefined,
      status: status || undefined,
      page,
      page_size: pageSize,
    },
  });

  const payload = response?.items ? response : response?.data?.items ? response.data : response || {};
  const items = Array.isArray(payload.items) ? payload.items : [];

  const mappedItems = items.map((item) => ({
    id: item.id,
    system: item.system || 'wparse',
    version: item.version,
    releaseGroup: item.release_group,
    status: item.status,
    pipeline: item.pipeline,
    owner: item.owner || item.created_by || '',
    stages: item.stages || [],
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    publishedAt: item.published_at,
    sandboxReady: item.sandbox_ready ?? false,
    // 还原资格由后端计算；只接受真正的布尔值，避免旧接口或异常数据
    // 返回字符串 "false" 时被 React 当成 truthy，错误显示还原按钮。
    canRestore: item.can_restore === true,
    restoreDisabledReason: item.restore_disabled_reason || '',
    restoreGroups: Array.isArray(item.restore_groups) ? item.restore_groups : [],
    latestRestore: item.latest_restore || null,
  }));

  return {
    items: mappedItems,
    total: payload.total ?? mappedItems.length,
    page: payload.page ?? page,
    pageSize: payload.page_size ?? pageSize,
  };
}

/**
 * 创建发布记录
 * @param {"models"|"infra"} releaseGroup
 * @param {string} [note]
 * @returns {Promise<Object>}
 */
export async function createRelease(releaseGroup, note, system) {
  const formattedNote =
    typeof note === 'string' && note.trim().length > 0 ? note.trim() : undefined;
  const response = await httpRequest.post('/releases', {
    system: resolveSystem(system),
    pipeline: formattedNote,
    note: formattedNote,
  });
  return typeof response?.success === 'boolean' ? response : response?.data || response;
}

/**
 * 获取发布详情（包含 devices 各机器发布情况）
 * @param {number|string} releaseId - 发布 ID
 * @returns {Promise<Object|null>} 发布详情
 */
export async function fetchReleaseDetail(releaseId, system) {
  const response = await httpRequest.get(`/releases/${releaseId}`, {
    params: { system: resolveSystem(system) },
  });
  return response?.id ? response : response?.data || response || null;
}

/**
 * 校验发布版本
 * @param {number|string} releaseId - 发布 ID
 * @returns {Promise<Object>} 校验结果
 */
export async function validateRelease(releaseId, system) {
  const response = await httpRequest.post(`/releases/${releaseId}/validate`, {
    system: resolveSystem(system),
    rule_type: 'all',
  });
  return response?.filename ? response : response?.data || response;
}

/**
 * 执行发布（多台机器）
 * @param {number|string} releaseId - 发布 ID
 * @param {number[]} deviceIds - 目标机器 ID 列表
 * @param {string} [note] - 发布备注
 * @returns {Promise<Object>} 发布结果
 */
export async function publishRelease(releaseId, releaseGroup, deviceIds = [], note, system) {
  const formattedNote =
    typeof note === 'string' && note.trim().length > 0 ? note.trim() : undefined;
  const response = await httpRequest.post(`/releases/${releaseId}/publish`, {
    system: resolveSystem(system),
    release_group: releaseGroup === 'all' ? 'models' : releaseGroup,
    full_publish: releaseGroup === 'all',
    device_ids: deviceIds,
    note: formattedNote,
  });
  return typeof response?.success === 'boolean' ? response : response?.data || response;
}

/**
 * 获取发布版本差异（git diff）
 * @param {number|string} releaseId - 发布 ID
 * @param {Object} [options]
 * @param {number} [options.offset=0]
 * @param {number} [options.limit=10]
 * @returns {Promise<Object>} 差异结果
 */
export async function fetchReleaseDiff(releaseId, options = {}, system) {
  const { offset = 0, limit = 10 } = options;
  const response = await httpRequest.get(`/releases/${releaseId}/diff`, {
    params: {
      system: resolveSystem(system),
      offset,
      limit,
    },
  });
  return response?.files ? response : response?.data || response;
}

/**
 * 回滚指定设备到上一个成功版本
 * @param {number|string} releaseId - 发布 ID
 * @param {number[]} deviceIds - 设备 ID 列表
 * @returns {Promise<Object>} 回滚结果
 */
export async function rollbackRelease(releaseId, deviceIds = [], targetIds = [], system) {
  const response = await httpRequest.post(`/releases/${releaseId}/rollback`, {
    system: resolveSystem(system),
    device_ids: deviceIds,
    target_ids: targetIds,
  });
  return typeof response?.success === 'boolean' ? response : response?.data || response;
}

/**
 * 以历史成功版本创建指定范围的还原发布任务。
 * @param {number|string} releaseId - 发布记录 ID
 * @param {"models"|"infra"|"all"} releaseGroup - 还原范围
 * @param {number[]} deviceIds - 目标设备 ID 列表
 * @param {string} [note] - 还原备注
 * @param {string} system - 当前系统
 * @returns {Promise<Object>} 还原结果
 */
export async function restoreRelease(releaseId, releaseGroup, deviceIds, note, system) {
  const username = sessionStorage.getItem('username');
  const response = await httpRequest.post(
    `/releases/${releaseId}/restore`,
    {
      system: resolveSystem(system),
      release_group: releaseGroup,
      device_ids: deviceIds,
      note: typeof note === 'string' && note.trim() ? note.trim() : undefined,
    },
    username ? { headers: { 'X-Operator': encodeURIComponent(username) } } : undefined,
  );
  return typeof response?.success === 'boolean' ? response : response?.data || response;
}

export async function fetchRestoreJob(jobId) {
  const response = await httpRequest.get(`/release-restores/${jobId}`);
  return response?.id ? response : response?.data || response;
}
