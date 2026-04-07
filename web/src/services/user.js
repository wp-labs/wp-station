/**
 * 用户管理服务模块
 * 提供用户列表查询、创建、编辑、状态管理、密码重置、删除功能
 * 对接后端 /api/users 相关接口
 */

import httpRequest from './request';

/**
 * 获取用户分页列表
 * @param {Object} options - 查询选项
 * @param {string} [options.keyword] - 搜索关键词（匹配用户名/显示名/邮箱）
 * @param {string} [options.role] - 角色筛选（admin / operator / viewer）
 * @param {string} [options.status] - 状态筛选（active / inactive）
 * @param {number} [options.page] - 页码，默认 1
 * @param {number} [options.pageSize] - 每页数量，默认 10
 * @returns {Promise<Object>} 分页结果 { items, total, page, pageSize }
 */
export async function fetchUsers(options = {}) {
  const { keyword, role, status, page = 1, pageSize = 10 } = options;

  const response = await httpRequest.get('/users', {
    params: {
      keyword: keyword || undefined,
      role: role || undefined,
      status: status || undefined,
      page,
      page_size: pageSize,
    },
  });

  const payload = response?.items ? response : response?.data || response || {};
  const items = Array.isArray(payload.items) ? payload.items : [];

  // 格式化时间为标准格式 YYYY-MM-DD HH:MM:SS
  const formatDateTime = (isoString) => {
    if (!isoString) return '';
    const date = new Date(isoString);
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    const hours = String(date.getHours()).padStart(2, '0');
    const minutes = String(date.getMinutes()).padStart(2, '0');
    const seconds = String(date.getSeconds()).padStart(2, '0');
    return `${year}-${month}-${day} ${hours}:${minutes}:${seconds}`;
  };

  // 将后端 snake_case 字段映射为前端 camelCase
  const mappedItems = items.map((item) => ({
    id: item.id,
    username: item.username,
    displayName: item.display_name || item.username,
    email: item.email || '',
    role: item.role,
    status: item.status,
    remark: item.remark || '',
    createdAt: formatDateTime(item.created_at),
    updatedAt: formatDateTime(item.updated_at),
  }));

  return {
    items: mappedItems,
    total: payload.total ?? mappedItems.length,
    page: payload.page ?? page,
    pageSize: payload.page_size ?? pageSize,
  };
}

/**
 * 创建用户
 * @param {Object} data - 用户数据
 * @param {string} data.username - 用户名
 * @param {string} data.password - 密码
 * @param {string} [data.displayName] - 显示名
 * @param {string} [data.email] - 邮箱
 * @param {string} data.role - 角色
 * @param {string} [data.remark] - 备注
 * @returns {Promise<Object>} 创建结果 { id }
 */
export async function createUser(data) {
  const response = await httpRequest.post('/users', {
    username: data.username,
    password: data.password,
    display_name: data.displayName,
    email: data.email,
    role: data.role,
    remark: data.remark,
  });
  return response?.id ? response : response?.data || response;
}

/**
 * 编辑用户基本信息
 * @param {number|string} userId - 用户 ID
 * @param {Object} data - 更新数据
 * @param {string} [data.displayName] - 显示名
 * @param {string} [data.email] - 邮箱
 * @param {string} [data.role] - 角色
 * @param {string} [data.remark] - 备注
 * @returns {Promise<void>}
 */
export async function updateUser(userId, data) {
  await httpRequest.put(`/users/${userId}`, {
    display_name: data.displayName,
    email: data.email,
    role: data.role,
    remark: data.remark,
  });
}

/**
 * 更新用户状态（启用 / 禁用）
 * @param {number|string} userId - 用户 ID
 * @param {string} status - 目标状态（active / inactive）
 * @returns {Promise<void>}
 */
export async function updateUserStatus(userId, status) {
  await httpRequest.put(`/users/${userId}/status`, { status });
}

/**
 * 重置用户密码（生成随机密码）
 * @param {number|string} userId - 用户 ID
 * @returns {Promise<Object>} { new_password: string }
 */
export async function resetUserPassword(userId) {
  const response = await httpRequest.post(`/users/${userId}/reset-password`, {});
  return response?.new_password ? response : response?.data || response;
}

/**
 * 修改用户密码
 * @param {number|string} userId - 用户 ID
 * @param {Object} data - 密码数据
 * @param {string} data.oldPassword - 旧密码
 * @param {string} data.newPassword - 新密码
 * @param {string} data.confirmPassword - 确认新密码
 * @returns {Promise<void>}
 */
export async function changeUserPassword(userId, data) {
  await httpRequest.post(`/users/${userId}/change-password`, {
    old_password: data.oldPassword,
    new_password: data.newPassword,
    confirm_password: data.confirmPassword,
  });
}

/**
 * 删除用户（软删除）
 * @param {number|string} userId - 用户 ID
 * @returns {Promise<void>}
 */
export async function deleteUser(userId) {
  await httpRequest.delete(`/users/${userId}`);
}
