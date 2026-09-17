/**
 * 特性配置相关 service
 * 目前提供数据采集监控地址的读取能力
 */

import httpRequest from './request';
import { resolveSystem } from './system';

/**
 * 获取数据采集页面配置
 * @returns {Promise<{data_collect_url: string, default_data_collect_url: string}>}
 */
export async function fetchDataCollectConfig() {
  const response = await httpRequest.get('/features/config');
  const payload = response?.data_collect_url ? response : response?.data || response || {};

  return {
    data_collect_url: payload.data_collect_url || payload.default_data_collect_url || '',
    default_data_collect_url: payload.default_data_collect_url || payload.data_collect_url || '',
  };
}

/**
 * 获取接入概览页的规则侧设备类型 / 日志类型摘要
 */
export async function fetchIntegrationRuleOverview(system) {
  const response = await httpRequest.get('/integration-overview/rules', {
    params: { system: resolveSystem(system) },
  });
  const payload = response?.items ? response : response?.data || response || {};

  const normalizeLogTypes = (items = []) =>
    (Array.isArray(items) ? items : []).map((item) => ({
      key: item?.key || '',
      logTypeName: item?.log_type_name || '',
      ruleKeys: Array.isArray(item?.rule_keys) ? item.rule_keys : [],
    }));

  const normalizeFlatItems = (items = []) =>
    (Array.isArray(items) ? items : []).map((item) => ({
      key: item?.key || '',
      name: item?.name || '',
      ruleNames: Array.isArray(item?.rule_names) ? item.rule_names : [],
    }));

  return {
    system: payload?.system || '',
    items: (Array.isArray(payload.items) ? payload.items : []).map((item) => ({
      key: item?.key || '',
      deviceType: item?.device_type || '',
      logTypes: normalizeLogTypes(item?.log_types),
    })),
    windowStructures: normalizeFlatItems(payload?.window_structures),
    associationRules: normalizeFlatItems(payload?.association_rules),
    windowStructureCount:
      typeof payload?.window_structure_count === 'number' ? payload.window_structure_count : 0,
    associationRuleCount:
      typeof payload?.association_rule_count === 'number' ? payload.association_rule_count : 0,
  };
}

/**
 * 获取接入概览页的输入源 / 输出源运行时摘要
 */
export async function fetchIntegrationRuntimeOverview(system) {
  const response = await httpRequest.get('/integration-overview/runtime', {
    params: { system: resolveSystem(system) },
  });
  const payload = response?.sources ? response : response?.data || response || {};

  const normalizeItems = (items = []) =>
    (Array.isArray(items) ? items : []).map((item) => ({
      key: item?.key || '',
      title: item?.title || '',
      connect: item?.connect || '',
      typeKey: item?.type_key || '',
      typeLabel: item?.type_label || '',
      detail: item?.detail || '-',
    }));

  return {
    sources: normalizeItems(payload.sources),
    sinks: normalizeItems(payload.sinks),
    supportedSourceTypeCount:
      typeof payload.supported_source_type_count === 'number'
        ? payload.supported_source_type_count
        : 0,
    supportedSinkTypeCount:
      typeof payload.supported_sink_type_count === 'number'
        ? payload.supported_sink_type_count
        : 0,
  };
}
