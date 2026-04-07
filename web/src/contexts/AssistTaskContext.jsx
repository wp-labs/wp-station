/**
 * AI 辅助任务全局 Context
 * 在 App 根级管理所有辅助任务的生命周期和轮询逻辑，不受页面切换影响
 * AI 分析和人工提单共用同一套状态管理
 */

import React, { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { App as AntdApp, Button } from 'antd';
import { useTranslation } from 'react-i18next';
import {
  cancelAssistTask,
  getAssistTask,
  submitAssistTask,
} from '@/services/assist_task';

// localStorage 持久化键名
const STORAGE_KEY = 'warpstation_assist_tasks';

// 轮询间隔策略（毫秒）
const POLL_INTERVALS = {
  AI_FAST: 3000,    // AI 任务前 60s：每 3s
  AI_SLOW: 10000,   // AI 任务 60s~5min：每 10s
  MANUAL_FAST: 30000, // 人工前 5min：每 30s
  MANUAL_SLOW: 60000, // 人工 5min 后：每 60s
};

// AI 任务超过 5 分钟停止轮询
const AI_TIMEOUT_MS = 5 * 60 * 1000;

const AssistTaskContext = createContext(null);

/**
 * 从 localStorage 加载持久化的任务列表
 * @returns {Array} 任务列表
 */
function loadTasksFromStorage() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

/**
 * 将任务列表持久化到 localStorage
 * @param {Array} tasks
 */
function saveTasksToStorage(tasks) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(tasks));
  } catch {
    // 忽略 storage 写入错误
  }
}

export function AssistTaskProvider({ children }) {
  const { t } = useTranslation();
  const { notification } = AntdApp.useApp();
  const navigate = useNavigate();

  // 所有任务状态（含已完成）
  const [allTasks, setAllTasks] = useState(() => loadTasksFromStorage());

  // 全局轮询定时器引用，避免内存泄漏
  const pollTimerRef = useRef(null);

  // 每个任务上次实际发请求的时间戳，useRef 持久化避免 Effect 重跑时被重置
  const lastPollTimeRef = useRef({});

  // 进行中的任务（pending / processing）
  const activeTasks = allTasks.filter(
    (task) => task.status === 'pending' || task.status === 'processing',
  );

  /**
   * 更新单条任务状态，同步到 state 和 localStorage
   * @param {string} taskId
   * @param {Object} updates - 需要合并的字段
   */
  const updateTask = useCallback((taskId, updates) => {
    setAllTasks((prevTasks) => {
      const nextTasks = prevTasks.map((task) =>
        task.task_id === taskId ? { ...task, ...updates } : task,
      );
      saveTasksToStorage(nextTasks);
      return nextTasks;
    });
  }, []);

  /**
   * 停止轮询指定任务，但保留任务记录供用户查看结果
   * @param {string} taskId
   */
  const stopTrackingTask = useCallback((taskId) => {
    delete lastPollTimeRef.current[taskId];
  }, []);

  /**
   * 删除任务（仅用于后端已不存在的本地缓存清理）
   * @param {string} taskId
   */
  const removeTask = useCallback((taskId) => {
    setAllTasks((prevTasks) => {
      const nextTasks = prevTasks.filter((task) => task.task_id !== taskId);
      saveTasksToStorage(nextTasks);
      return nextTasks;
    });
    stopTrackingTask(taskId);
  }, [stopTrackingTask]);

  /**
   * 对单个任务执行一次轮询，根据返回状态决定是否停止
   * @param {Object} task - 当前任务对象
   */
  const pollSingleTask = useCallback(
    async (task) => {
      try {
        const latest = await getAssistTask(task.task_id);

        if (latest.status === 'success') {
          // 任务成功：更新状态并发送全局通知
          updateTask(task.task_id, latest);
          stopTrackingTask(task.task_id);

          // 构造通知中的"查看并填充"按钮
          const fillButton = (
            <Button
              type="primary"
              size="small"
              onClick={() => {
                navigate(`/simulate-debug?assistTaskId=${task.task_id}`);
              }}
            >
              {t('assistTask.viewAndFill')}
            </Button>
          );

          notification.success({
            key: `assist-done-${task.task_id}`,
            message: t('assistTask.taskCompleted'),
            description: t(
              task.task_type === 'ai'
                ? 'assistTask.aiCompleted'
                : 'assistTask.manualCompleted',
            ),
            btn: fillButton,
            duration: 5,
          });
        } else if (latest.status === 'error' || latest.status === 'cancelled') {
          // 任务失败或取消：更新状态，发送错误通知
          updateTask(task.task_id, latest);
          stopTrackingTask(task.task_id);

          if (latest.status === 'error') {
            notification.error({
              key: `assist-error-${task.task_id}`,
              message: t('assistTask.taskFailed'),
              description: latest.error_message || t('assistTask.unknownError'),
              duration: 8,
            });
          }
        } else {
          // 仍在进行中，只更新等待时间等字段
          updateTask(task.task_id, {
            status: latest.status,
            wait_seconds: latest.wait_seconds,
            updated_at: latest.updated_at,
          });

          // AI 任务超时检查：使用后端返回的 wait_seconds，避免重启后用旧 created_at 误判
          if (task.task_type === 'ai') {
            const waitMs = (latest.wait_seconds ?? 0) * 1000;
            if (waitMs > AI_TIMEOUT_MS) {
              updateTask(task.task_id, {
                status: 'error',
                error_message: t('assistTask.aiTimeout'),
              });
              notification.error({
                key: `assist-timeout-${task.task_id}`,
                message: t('assistTask.taskFailed'),
                description: t('assistTask.aiTimeout'),
                duration: 8,
              });
            }
          }
        }
      } catch (error) {
        const statusCode = error?.response?.status;
        if (statusCode === 404) {
          // 后端已删除任务，直接清理本地缓存，避免持续轮询
          removeTask(task.task_id);
          return;
        }
        // 网络错误不中断轮询，静默忽略
        console.warn(`辅助任务轮询失败: task_id=${task.task_id}`, error);
      }
    },
    [navigate, notification, removeTask, stopTrackingTask, t, updateTask],
  );

  /**
   * 计算当前任务的合适轮询间隔（智能降频）
   * @param {Object} task
   * @returns {number} 毫秒
   */
  const getTaskPollInterval = useCallback((task) => {
    const elapsed = Date.now() - new Date(task.created_at).getTime();
    if (task.task_type === 'ai') {
      return elapsed < 60000 ? POLL_INTERVALS.AI_FAST : POLL_INTERVALS.AI_SLOW;
    }
    // 人工任务
    return elapsed < 5 * 60 * 1000 ? POLL_INTERVALS.MANUAL_FAST : POLL_INTERVALS.MANUAL_SLOW;
  }, []);

  /**
   * 启动全局轮询循环
   * 每 3s 检查一次进行中任务，按各自策略决定是否实际发请求
   */
  useEffect(() => {
    // 没有进行中任务时停止轮询
    if (activeTasks.length === 0) {
      if (pollTimerRef.current) {
        clearInterval(pollTimerRef.current);
        pollTimerRef.current = null;
      }
      return;
    }

    // 避免重复创建定时器
    if (pollTimerRef.current) {
      clearInterval(pollTimerRef.current);
    }

    pollTimerRef.current = setInterval(() => {
      const now = Date.now();
      activeTasks.forEach((task) => {
        const interval = getTaskPollInterval(task);
        // 使用 ref 持久化上次轮询时间，避免 Effect 重跑时被重置导致间隔失效
        const lastTime = lastPollTimeRef.current[task.task_id] || 0;

        if (now - lastTime >= interval) {
          lastPollTimeRef.current[task.task_id] = now;
          pollSingleTask(task);
        }
      });
    }, 3000); // 每 3s 检查一次各任务是否需要轮询

    return () => {
      if (pollTimerRef.current) {
        clearInterval(pollTimerRef.current);
        pollTimerRef.current = null;
      }
    };
  }, [activeTasks, getTaskPollInterval, pollSingleTask]);

  /**
   * 提交辅助任务
   * @param {Object} options - submitAssistTask 的参数
   * @returns {Promise<{ task_id: string, status: string }>}
   */
  const submitTask = useCallback(
    async (options) => {
      const result = await submitAssistTask(options);

      if (result.task_id) {
        const newTask = {
          task_id: result.task_id,
          task_type: options.taskType,
          target_rule: options.targetRule,
          status: 'pending',
          log_data: options.logData,
          wpl_suggestion: null,
          oml_suggestion: null,
          explanation: null,
          error_message: null,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          wait_seconds: 0,
        };

        setAllTasks((prevTasks) => {
          const nextTasks = [newTask, ...prevTasks];
          saveTasksToStorage(nextTasks);
          return nextTasks;
        });
      }

      return result;
    },
    [],
  );

  /**
   * 取消任务
   * @param {string} taskId
   */
  const cancelTask = useCallback(
    async (taskId) => {
      await cancelAssistTask(taskId);
      updateTask(taskId, { status: 'cancelled' });
    },
    [updateTask],
  );

  /**
   * 根据 task_id 从列表中查找任务
   * @param {string} taskId
   * @returns {Object|null}
   */
  const getTaskById = useCallback(
    (taskId) => allTasks.find((task) => task.task_id === taskId) || null,
    [allTasks],
  );

  /**
   * 清除已完成/失败/取消的任务记录
   */
  const clearFinishedTasks = useCallback(() => {
    setAllTasks((prevTasks) => {
      const nextTasks = prevTasks.filter(
        (task) => task.status === 'pending' || task.status === 'processing',
      );
      saveTasksToStorage(nextTasks);
      return nextTasks;
    });
  }, []);

  const contextValue = {
    allTasks,
    activeTasks,
    submitTask,
    cancelTask,
    getTaskById,
    clearFinishedTasks,
  };

  return (
    <AssistTaskContext.Provider value={contextValue}>
      {children}
    </AssistTaskContext.Provider>
  );
}

/**
 * 使用 AssistTask Context 的 hook
 * @returns {Object} context value
 */
export function useAssistTask() {
  const context = useContext(AssistTaskContext);
  if (!context) {
    throw new Error('useAssistTask 必须在 AssistTaskProvider 内使用');
  }
  return context;
}

export default AssistTaskContext;
