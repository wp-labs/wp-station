import httpRequest from './request';

/**
 * 从 project_root 读取文件并初始化数据库配置
 * @returns {Promise<Object>} 导入结果
 */
export async function importProjectFromFiles() {
  const response = await httpRequest.post('/project/import');
  return response;
}
