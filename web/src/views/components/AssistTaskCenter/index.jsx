/**
 * AI 辅助任务中心组件
 * 全局悬浮按钮 + 任务列表弹窗，不绑定任何具体页面
 * 用户可随时查看所有任务进展、复制结果、或跳转填充
 */

import {
  CheckCircleOutlined,
  ClockCircleOutlined,
  CloseCircleOutlined,
  CopyOutlined,
  RobotOutlined,
  SyncOutlined,
  UserOutlined,
} from '@ant-design/icons';
import { Button, Collapse, FloatButton, Modal, Space, Tag, Tooltip, Typography, message } from 'antd';
import React, { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { useAssistTask } from '@/contexts/AssistTaskContext';

const { Text, Paragraph } = Typography;

/**
 * 格式化等待时长为可读字符串
 * @param {number} seconds
 * @returns {string}
 */
function formatWaitTime(seconds) {
  if (!seconds || seconds < 60) {
    return `${seconds || 0}s`;
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  }
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
}

/**
 * 任务状态标签配置
 */
function TaskStatusTag({ status }) {
  const { t } = useTranslation();

  const statusConfig = {
    pending: {
      color: 'processing',
      icon: <ClockCircleOutlined />,
      label: t('assistTask.status.pending'),
    },
    processing: {
      color: 'processing',
      icon: <SyncOutlined spin />,
      label: t('assistTask.status.processing'),
    },
    success: {
      color: 'success',
      icon: <CheckCircleOutlined />,
      label: t('assistTask.status.success'),
    },
    error: {
      color: 'error',
      icon: <CloseCircleOutlined />,
      label: t('assistTask.status.error'),
    },
    cancelled: {
      color: 'default',
      icon: <CloseCircleOutlined />,
      label: t('assistTask.status.cancelled'),
    },
  };

  const config = statusConfig[status] || statusConfig.pending;
  return (
    <Tag color={config.color} icon={config.icon}>
      {config.label}
    </Tag>
  );
}

/**
 * 任务详情展开面板内容
 * @param {Object} task - 任务对象
 * @param {number} displayWaitSeconds - 实时递增的等待秒数（由父组件传入）
 * @param {Function} onFillClick - 点击"查看并填充"回调
 */
function TaskDetailContent({ task, displayWaitSeconds, onFillClick }) {
  const { t } = useTranslation();
  const [messageApi, contextHolder] = message.useMessage();

  const handleCopy = (text, label) => {
    navigator.clipboard.writeText(text).then(() => {
      messageApi.success(t('assistTask.copySuccess', { label }));
    });
  };

  return (
    <div style={{ padding: '4px 0' }}>
      {contextHolder}

      {task.status === 'error' && task.error_message && (
        <div style={{ marginBottom: 12 }}>
          <Text type="danger">{task.error_message}</Text>
        </div>
      )}

      {task.status === 'success' && (
        <>
          {task.explanation && (
            <div style={{ marginBottom: 12 }}>
              <Text type="secondary" style={{ fontSize: 12 }}>
                {t('assistTask.explanation')}
              </Text>
              <Paragraph
                style={{
                  background: '#f5f5f5',
                  padding: '8px 12px',
                  borderRadius: 8,
                  marginTop: 4,
                  marginBottom: 0,
                  fontSize: 13,
                  whiteSpace: 'pre-wrap',
                }}
              >
                {task.explanation}
              </Paragraph>
            </div>
          )}

          {task.wpl_suggestion && (
            <div style={{ marginBottom: 12 }}>
              <Space style={{ marginBottom: 4 }}>
                <Text type="secondary" style={{ fontSize: 12 }}>
                  {t('assistTask.wplSuggestion')}
                </Text>
                <Tooltip title={t('assistTask.copy')}>
                  <Button
                    size="small"
                    icon={<CopyOutlined />}
                    onClick={() => handleCopy(task.wpl_suggestion, 'WPL')}
                  />
                </Tooltip>
              </Space>
              <Paragraph
                code
                style={{
                  background: '#1e1e2e',
                  color: '#cdd6f4',
                  padding: '8px 12px',
                  borderRadius: 8,
                  marginBottom: 0,
                  fontSize: 12,
                  maxHeight: 120,
                  overflowY: 'auto',
                  whiteSpace: 'pre',
                }}
              >
                {task.wpl_suggestion}
              </Paragraph>
            </div>
          )}

          {task.oml_suggestion && (
            <div style={{ marginBottom: 12 }}>
              <Space style={{ marginBottom: 4 }}>
                <Text type="secondary" style={{ fontSize: 12 }}>
                  {t('assistTask.omlSuggestion')}
                </Text>
                <Tooltip title={t('assistTask.copy')}>
                  <Button
                    size="small"
                    icon={<CopyOutlined />}
                    onClick={() => handleCopy(task.oml_suggestion, 'OML')}
                  />
                </Tooltip>
              </Space>
              <Paragraph
                code
                style={{
                  background: '#1e1e2e',
                  color: '#cdd6f4',
                  padding: '8px 12px',
                  borderRadius: 8,
                  marginBottom: 0,
                  fontSize: 12,
                  maxHeight: 120,
                  overflowY: 'auto',
                  whiteSpace: 'pre',
                }}
              >
                {task.oml_suggestion}
              </Paragraph>
            </div>
          )}

          <Button
            type="primary"
            size="small"
            style={{ marginTop: 4 }}
            onClick={() => onFillClick(task.task_id)}
          >
            {t('assistTask.viewAndFill')}
          </Button>
        </>
      )}

      {(task.status === 'pending' || task.status === 'processing') && (
        <Text type="secondary" style={{ fontSize: 12 }}>
          {t('assistTask.waitingDesc', { time: formatWaitTime(displayWaitSeconds) })}
        </Text>
      )}
    </div>
  );
}

/**
 * 辅助任务中心主组件
 * 只在登录后渲染（由 App.jsx 在 RequireAuth 内部挂载）
 */
function AssistTaskCenter() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { allTasks, activeTasks, cancelTask } = useAssistTask();

  const [isModalOpen, setIsModalOpen] = useState(false);

  // 追踪已读的完成任务 ID，用于判断是否有未读的新结果
  const [readTaskIds, setReadTaskIds] = useState(() => {
    try {
      const raw = localStorage.getItem('warpstation_assist_read_ids');
      return raw ? new Set(JSON.parse(raw)) : new Set();
    } catch {
      return new Set();
    }
  });

  // 计算未读的完成任务（success / error）
  const unreadDoneTasks = allTasks.filter(
    (task) =>
      (task.status === 'success' || task.status === 'error') &&
      !readTaskIds.has(task.task_id),
  );

  // 打开弹窗时标记所有完成任务为已读
  const handleOpenModal = () => {
    const doneIds = allTasks
      .filter((task) => task.status === 'success' || task.status === 'error')
      .map((task) => task.task_id);
    const next = new Set([...readTaskIds, ...doneIds]);
    setReadTaskIds(next);
    try {
      localStorage.setItem('warpstation_assist_read_ids', JSON.stringify([...next]));
    } catch {
      // 忽略
    }
    setIsModalOpen(true);
  };

  // 本地递增的等待秒数，key 为 task_id，每秒 +1，仅对进行中任务生效
  // 避免依赖轮询刷新（人工任务 30s 才轮询一次，界面会长时间静止）
  const [localWaitSeconds, setLocalWaitSeconds] = useState({});
  const localTimerRef = useRef(null);

  useEffect(() => {
    if (activeTasks.length === 0) {
      setLocalWaitSeconds({});
      if (localTimerRef.current) {
        clearInterval(localTimerRef.current);
        localTimerRef.current = null;
      }
      return;
    }

    // 初始化：以后端快照值为基准
    setLocalWaitSeconds((prev) => {
      const next = { ...prev };
      activeTasks.forEach((task) => {
        if (next[task.task_id] === undefined) {
          next[task.task_id] = task.wait_seconds ?? 0;
        }
      });
      return next;
    });

    if (localTimerRef.current) clearInterval(localTimerRef.current);
    localTimerRef.current = setInterval(() => {
      setLocalWaitSeconds((prev) => {
        const next = { ...prev };
        activeTasks.forEach((task) => {
          next[task.task_id] = (next[task.task_id] ?? task.wait_seconds ?? 0) + 1;
        });
        return next;
      });
    }, 1000);

    return () => {
      if (localTimerRef.current) {
        clearInterval(localTimerRef.current);
        localTimerRef.current = null;
      }
    };
  }, [activeTasks]);

  const handleFillClick = (taskId) => {
    setIsModalOpen(false);
    navigate(`/simulate-debug?assistTaskId=${taskId}`);
  };

  // 构建 Collapse items，每个任务一条
  const collapseItems = allTasks.map((task) => {
    const typeIcon = task.task_type === 'ai' ? <RobotOutlined /> : <UserOutlined />;
    const typeLabel = t(`assistTask.taskType.${task.task_type}`);
    const timeLabel = new Date(task.created_at).toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });

    const headerExtra = (
      <Space size={4} onClick={(event) => event.stopPropagation()}>
        <TaskStatusTag status={task.status} />
        {(task.status === 'pending' || task.status === 'processing') && (
          <Tooltip title={t('assistTask.cancel')}>
            <Button
              size="small"
              danger
              onClick={() => cancelTask(task.task_id)}
            >
              {t('assistTask.cancel')}
            </Button>
          </Tooltip>
        )}
      </Space>
    );

    return {
      key: task.task_id,
      label: (
        <Space>
          {typeIcon}
          <Text strong style={{ fontSize: 13 }}>
            {typeLabel}
          </Text>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {timeLabel}
          </Text>
          {(task.status === 'pending' || task.status === 'processing') && (
            <Text type="secondary" style={{ fontSize: 12 }}>
              · {t('assistTask.waited')} {formatWaitTime(localWaitSeconds[task.task_id] ?? task.wait_seconds)}
            </Text>
          )}
        </Space>
      ),
      extra: headerExtra,
      children: (
        <TaskDetailContent
          task={task}
          displayWaitSeconds={localWaitSeconds[task.task_id] ?? task.wait_seconds}
          onFillClick={handleFillClick}
        />
      ),
    };
  });

  // 主按钮角标：优先展示未读完成任务数，其次展示进行中任务数
  const mainBadge = unreadDoneTasks.length > 0
    ? { count: unreadDoneTasks.length }
    : activeTasks.length > 0
      ? { count: activeTasks.length }
      : undefined;

  return (
    <>
      {/* 悬浮按钮：任务中心，角标优先展示未读完成任务数，其次为进行中任务数 */}
      <FloatButton
        style={{ bottom: 80, right: 24 }}
        icon={<RobotOutlined />}
        tooltip={t('assistTask.taskCenter')}
        badge={mainBadge}
        onClick={handleOpenModal}
      />

      {/* 任务列表弹窗 */}
      <Modal
        title={
          <Space>
            <RobotOutlined />
            {t('assistTask.taskCenter')}
          </Space>
        }
        open={isModalOpen}
        onCancel={() => setIsModalOpen(false)}
        footer={null}
        width={640}
        styles={{ body: { maxHeight: '60vh', overflowY: 'auto' } }}
      >
        {allTasks.length === 0 ? (
          <div style={{ textAlign: 'center', padding: '32px 0', color: '#999' }}>
            {t('assistTask.noTasks')}
          </div>
        ) : (
          <Collapse
            items={collapseItems}
            accordion={false}
            style={{ background: 'transparent' }}
          />
        )}
      </Modal>
    </>
  );
}

export default AssistTaskCenter;
