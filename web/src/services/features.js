/**
 * 特性配置相关 service
 * 目前提供数据采集监控地址的读取能力
 */

import httpRequest from './request';

const DEFAULT_DATA_COLLECT_URL = 'http://localhost:18080/wp-monitor';

/**
 * 获取数据采集页面配置
 * @returns {Promise<{data_collect_url: string}>}
 */
export async function fetchDataCollectConfig() {
  const response = await httpRequest.get('/features/config');
  const payload = response?.data_collect_url ? response : response?.data || response || {};

  return {
    data_collect_url: payload.data_collect_url || DEFAULT_DATA_COLLECT_URL,
  };
}

export { DEFAULT_DATA_COLLECT_URL };
