/**
 * 沙盒运行服务
 * 负责与 /api/sandbox 相关接口交互
 */

import httpRequest from './request';

/**
 * 创建一次沙盒运行任务
 * @param {Object} payload
 * @returns {Promise<Object>}
 */
export async function createSandboxRun(payload) {
  const response = await httpRequest.post('/sandbox/runs', payload);
  return response?.task_id ? response : response?.data || response;
}

/**
 * 查询沙盒任务详情
 * @param {string} taskId
 * @returns {Promise<Object>}
 */
export async function fetchSandboxRun(taskId) {
  if (!taskId) {
    return null;
  }
  const response = await httpRequest.get(`/sandbox/runs/${taskId}`);
  return response?.task_id ? response : response?.data || response;
}

/**
 * 停止沙盒任务
 * @param {string} taskId
 * @returns {Promise<Object>}
 */
export async function stopSandboxRun(taskId) {
  const response = await httpRequest.post(`/sandbox/runs/${taskId}/stop`);
  return response?.task_id ? response : response?.data || response;
}

/**
 * 获取阶段日志内容
 * @param {string} taskId
 * @param {string} stage
 * @returns {Promise<Object>}
 */
export async function fetchSandboxStageLogs(taskId, stage) {
  const response = await httpRequest.get(`/sandbox/runs/${taskId}/logs/${stage}`);
  return response?.stage ? response : response?.data || response;
}

/**
 * 查询某个发布最近一次沙盒结果
 * @param {number|string} releaseId
 * @returns {Promise<Object|null>}
 */
export async function fetchLatestSandboxRun(releaseId) {
  if (!releaseId) {
    return null;
  }
  const response = await httpRequest.get(`/releases/${releaseId}/sandbox/latest`);
  return response?.task_id ? response : response?.data || response;
}

/**
 * 获取沙盒历史记录
 * @param {number|string} releaseId
 * @param {number} limit
 * @returns {Promise<Object>}
 */
export async function fetchSandboxHistory(releaseId, limit = 20) {
  if (!releaseId) {
    return { total: 0, items: [] };
  }
  const response = await httpRequest.get(`/releases/${releaseId}/sandbox/runs`, {
    params: { limit },
  });
  const data = response?.items ? response : response?.data || response;
  return data ?? { total: 0, items: [] };
}
