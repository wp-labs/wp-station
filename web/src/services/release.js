/**
 * 系统发布服务模块
 * 提供发布列表查询、发布详情查询、校验和发布功能
 * 已接入后端 /api/releases 相关接口
 */

import httpRequest from './request';

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
  const { version, pipeline, createdBy, status, page = 1, pageSize = 10 } = options;

  const response = await httpRequest.get('/releases', {
    params: {
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
    version: item.version,
    status: item.status,
    pipeline: item.pipeline,
    owner: item.owner || item.created_by || '',
    stages: item.stages || [],
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    publishedAt: item.published_at,
    sandboxReady: item.sandbox_ready ?? false,
  }));

  return {
    items: mappedItems,
    total: payload.total ?? mappedItems.length,
    page: payload.page ?? page,
    pageSize: payload.page_size ?? pageSize,
  };
}

/**
 * 获取发布详情（包含 devices 各机器发布情况）
 * @param {number|string} releaseId - 发布 ID
 * @returns {Promise<Object|null>} 发布详情
 */
export async function fetchReleaseDetail(releaseId) {
  const response = await httpRequest.get(`/releases/${releaseId}`);
  return response?.id ? response : response?.data || response || null;
}

/**
 * 校验发布版本
 * @param {number|string} releaseId - 发布 ID
 * @returns {Promise<Object>} 校验结果
 */
export async function validateRelease(releaseId) {
  const response = await httpRequest.post(`/releases/${releaseId}/validate`, {
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
export async function publishRelease(releaseId, deviceIds = [], note) {
  const formattedNote =
    typeof note === 'string' && note.trim().length > 0 ? note.trim() : undefined;
  const response = await httpRequest.post(`/releases/${releaseId}/publish`, {
    device_ids: deviceIds,
    note: formattedNote,
  });
  return typeof response?.success === 'boolean' ? response : response?.data || response;
}

/**
 * 获取发布版本差异（git diff）
 * @param {number|string} releaseId - 发布 ID
 * @returns {Promise<Object>} 差异结果 { files: [], stats: {} }
 */
export async function fetchReleaseDiff(releaseId) {
  const response = await httpRequest.get(`/releases/${releaseId}/diff`);
  return response?.files ? response : response?.data || response;
}

/**
 * 回滚指定设备到上一个成功版本
 * @param {number|string} releaseId - 发布 ID
 * @param {number[]} deviceIds - 设备 ID 列表
 * @returns {Promise<Object>} 回滚结果
 */
export async function rollbackRelease(releaseId, deviceIds = []) {
  const response = await httpRequest.post(`/releases/${releaseId}/rollback`, {
    device_ids: deviceIds,
  });
  return typeof response?.success === 'boolean' ? response : response?.data || response;
}
