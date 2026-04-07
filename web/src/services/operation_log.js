/**
 * 操作日志服务模块
 * 提供操作日志分页查询功能
 * 对接后端 GET /api/operation-logs
 */

import httpRequest from './request';

/**
 * 获取操作日志分页列表
 * @param {Object} options - 查询选项
 * @param {string} [options.operator] - 操作人模糊匹配
 * @param {string} [options.operation] - 操作类型（create / update / delete / publish）
 * @param {string} [options.startDate] - 开始日期，格式 YYYY-MM-DD
 * @param {string} [options.endDate] - 结束日期，格式 YYYY-MM-DD
 * @param {number} [options.page] - 页码，默认 1
 * @param {number} [options.pageSize] - 每页数量，默认 10
 * @returns {Promise<Object>} 分页结果 { items, total, page, pageSize }
 */
export async function fetchOperationLogs(options = {}) {
  const { operator, operation, startDate, endDate, page = 1, pageSize = 10 } = options;

  const response = await httpRequest.get('/operation-logs', {
    params: {
      operator: operator || undefined,
      operation: operation || undefined,
      start_date: startDate || undefined,
      end_date: endDate || undefined,
      page,
      page_size: pageSize,
    },
  });

  const payload = response?.items ? response : response?.data || response || {};
  const items = Array.isArray(payload.items) ? payload.items : [];

  // 将后端 snake_case 字段映射为前端 camelCase
  const mappedItems = items.map((item) => ({
    id: item.id,
    operator: item.operator,
    operation: item.operation,
    target: item.target || '',
    description: item.description || '',
    content: item.content || '',
    status: item.status,
    updatedAt: item.updated_at,
  }));

  return {
    items: mappedItems,
    total: payload.total ?? mappedItems.length,
    page: payload.page ?? page,
    pageSize: payload.page_size ?? pageSize,
  };
}
