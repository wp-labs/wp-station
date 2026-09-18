/**
 * 连接管理服务模块
 * 提供连接列表查询、在线机器查询、增删改功能
 */

import httpRequest from './request';
import { resolveSystem } from './system';

/**
 * 获取连接列表（分页）
 * @param {Object} [options] - 查询选项
 * @param {string} [options.keyword] - 关键字（名称 / IP / 备注 模糊匹配）
 * @param {number} [options.page] - 页码（从 1 开始）
 * @param {number} [options.pageSize] - 每页条数
 * @returns {Promise<Object>} 分页结果
 */
export async function fetchConnections(options = {}) {
  const { keyword, page, pageSize, system } = options;

  const params = { system: resolveSystem(system) };
  if (keyword) params.keyword = keyword;
  if (typeof page === 'number') params.page = page;
  if (typeof pageSize === 'number') params.page_size = pageSize;

  const response = await httpRequest.get('/devices', {
    params: Object.keys(params).length > 0 ? params : undefined,
  });

  let rawItems = [];
  let total = 0;
  let currentPage = typeof page === 'number' ? page : 1;
  let currentPageSize = pageSize;

  if (Array.isArray(response)) {
    rawItems = response;
    total = response.length;
  } else if (response && typeof response === 'object') {
    if (Array.isArray(response.items)) rawItems = response.items;
    if (typeof response.total === 'number') total = response.total;
    if (typeof response.page === 'number') currentPage = response.page;
    if (typeof response.page_size === 'number') currentPageSize = response.page_size;
  }

  const items = rawItems.map((conn) => ({
    id: conn.id,
    name: conn.name || '',
    ip: conn.ip || '',
    port: conn.port,
    token: conn.token || '',
    remark: conn.remark || '',
    status: conn.status || 'inactive',
    client_version: conn.client_version || null,
    config_version: conn.config_version || null,
    healthError: conn.health_error || null,
    createdAt: conn.created_at,
    updatedAt: conn.updated_at,
  }));

  return {
    items,
    total: total || items.length,
    page: currentPage,
    pageSize: currentPageSize || pageSize || items.length || 10,
  };
}

/**
 * 获取在线连接列表（用于发布时多选目标机器）
 * @returns {Promise<Array>} 在线连接数组
 */
export async function fetchOnlineConnections(system) {
  const response = await httpRequest.get('/devices/online', {
    params: { system: resolveSystem(system) },
  });
  const rawItems = Array.isArray(response) ? response : (response?.items || []);
  return rawItems.map((conn) => ({
    id: conn.id,
    name: conn.name || '',
    ip: conn.ip || '',
    port: conn.port,
    remark: conn.remark || '',
    status: conn.status || 'active',
    configVersion: conn.config_version || conn.configVersion || '',
  }));
}

/**
 * 创建连接
 * @param {Object} options - 创建参数
 * @param {string} [options.name] - 连接名称
 * @param {string} options.ip - IP 地址
 * @param {number} options.port - 端口
 * @param {string} [options.remark] - 备注
 * @returns {Promise<Object>} 创建结果，包含新建 id
 */
export async function createConnection(options) {
  const { name, ip, port, token, remark, system } = options;
  return httpRequest.post('/devices', {
    system: resolveSystem(system),
    name: name || undefined,
    ip,
    port: Number(port),
    token,
    remark: remark || undefined,
  });
}

/**
 * 更新连接
 * @param {Object} options - 更新参数
 * @param {number} options.id - 连接 ID
 * @param {string} [options.name] - 连接名称
 * @param {string} options.ip - IP 地址
 * @param {number} options.port - 端口
 * @param {string} [options.remark] - 备注
 * @returns {Promise<Object>} 更新结果
 */
export async function updateConnection(options) {
  const { id, name, ip, port, token, remark, system } = options;
  return httpRequest.put('/devices', {
    id,
    system: resolveSystem(system),
    name: name || undefined,
    ip,
    port: Number(port),
    token,
    remark: remark || undefined,
  });
}

/**
 * 删除连接
 * @param {Object} options - 删除参数
 * @param {number} options.id - 连接 ID
 * @returns {Promise<Object>} 删除结果
 */
export async function deleteConnection(options) {
  const { id } = options;
  return httpRequest.delete(`/devices/${id}`);
}

/**
 * 手动刷新连接状态
 * @param {number} id - 连接 ID
 * @returns {Promise<Object>} 最新连接信息
 */
export async function refreshConnectionStatus(id) {
  const response = await httpRequest.post(`/devices/${id}/refresh`);
  const payload = response?.data || response;
  if (payload?.device) {
    return {
      device: {
        id: payload.device.id,
        name: payload.device.name || '',
        ip: payload.device.ip || '',
        port: payload.device.port,
        token: payload.device.token || '',
        remark: payload.device.remark || '',
        status: payload.device.status || 'inactive',
        client_version: payload.device.client_version || null,
        config_version: payload.device.config_version || null,
        healthError: payload.health_error || null,
        createdAt: payload.device.created_at,
        updatedAt: payload.device.updated_at,
      },
      healthError: payload.health_error || null,
    };
  }
  return payload;
}
