/**
 * AI 辅助规则编写服务模块
 * AI 分析和人工提单共用同一套接口，Station 统一存储任务状态
 * 结果均通过 reply 接口写回，前端轮询 getAssistTask 获取最新状态
 */

import httpRequest from './request';

/**
 * 提交辅助任务（AI 分析或人工提单）
 * @param {Object} options
 * @param {'ai'|'manual'} options.taskType - 任务类型
 * @param {'wpl'|'oml'|'both'} options.targetRule - 目标规则类型
 * @param {string} options.logData - 用户日志数据
 * @param {string} [options.currentRule] - 当前已有规则（供 AI 参考）
 * @param {string} [options.extraNote] - 用户补充说明（仅 manual）
 * @returns {Promise<{ task_id: string, status: string }>}
 */
export async function submitAssistTask(options = {}) {
  const { taskType, targetRule, logData, currentRule, extraNote } = options;

  const response = await httpRequest.post('/assist', {
    task_type: taskType,
    target_rule: targetRule,
    log_data: logData,
    current_rule: currentRule || undefined,
    extra_note: extraNote || undefined,
  });

  return response?.data ?? response ?? {};
}

/**
 * 查询单个辅助任务状态和结果（前端轮询此接口）
 * @param {string} taskId
 * @returns {Promise<AssistTaskDetail>}
 */
export async function getAssistTask(taskId) {
  const response = await httpRequest.get(`/assist/${taskId}`);
  return response?.data ?? response ?? {};
}

/**
 * 分页查询辅助任务列表
 * @param {Object} options
 * @param {number} [options.page] - 页码，默认 1
 * @param {number} [options.pageSize] - 每页数量，默认 10
 * @returns {Promise<{ items: AssistTaskDetail[], total: number, page: number, page_size: number }>}
 */
export async function listAssistTasks(options = {}) {
  const { page = 1, pageSize = 10 } = options;

  const response = await httpRequest.get('/assist', {
    params: { page, page_size: pageSize },
  });

  return response?.data ?? response ?? {};
}

/**
 * 取消等待中的辅助任务
 * @param {string} taskId
 * @returns {Promise<{ success: boolean }>}
 */
export async function cancelAssistTask(taskId) {
  const response = await httpRequest.post(`/assist/${taskId}/cancel`);
  return response?.data ?? response ?? {};
}

/**
 * 写回辅助任务结果（供调试/测试使用，生产由远端服务调用）
 * task_id 写在请求体中，不在 URL 路径
 * @param {string} taskId
 * @param {Object} options
 * @param {string} [options.wplSuggestion] - 建议的 WPL 规则
 * @param {string} [options.omlSuggestion] - 建议的 OML 规则
 * @param {string} [options.explanation] - 分析说明
 * @returns {Promise<{ success: boolean }>}
 */
export async function replyAssistTask(taskId, options = {}) {
  const { wplSuggestion, omlSuggestion, explanation } = options;

  const response = await httpRequest.post('/assist/reply', {
    task_id: taskId,
    wpl_suggestion: wplSuggestion || undefined,
    oml_suggestion: omlSuggestion || undefined,
    explanation: explanation || undefined,
  });

  return response?.data ?? response ?? {};
}
