/**
 * 身份验证服务模块
 * 提供登录、登出和会话管理功能
 */

import httpRequest from './request';

/**
 * 用户登录
 * @param {Object} credentials - 登录凭证
 * @param {string} credentials.username - 用户名
 * @param {string} credentials.password - 密码
 * @returns {Promise<Object>} 登录结果 { token, username, display_name, role }
 */
export async function login(credentials) {
  const response = await httpRequest.post('/auth/login', {
    username: credentials.username,
    password: credentials.password,
  });
  
  const result = response?.token ? response : response?.data || response;
  
  // 存储登录状态到 sessionStorage
  sessionStorage.setItem('isLoggedIn', '1');
  sessionStorage.setItem('username', result.display_name || result.username);
  sessionStorage.setItem('token', result.token);
  sessionStorage.setItem('role', result.role);

  return result;
}

/**
 * 用户登出
 * 清除所有会话数据
 */
export function logout() {
  sessionStorage.clear();
}

/**
 * 获取当前会话用户信息
 * @returns {Object} 用户信息
 * @returns {boolean} return.isLoggedIn - 是否已登录
 * @returns {string} return.username - 用户名
 */
export function getSessionUser() {
  const isLoggedIn = sessionStorage.getItem('isLoggedIn') === '1';
  const username = sessionStorage.getItem('username') || '';
  
  return {
    isLoggedIn,
    username,
  };
}
