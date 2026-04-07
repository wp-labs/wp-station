/**
 * AI 辅助结果展示抽屉
 * 从底部滑出，不遮挡编辑区，用户可对照结果进行调整
 * 支持一键填入全部区域（WPL + OML）并自动执行解析和转换
 */

import { CheckOutlined, CopyOutlined } from '@ant-design/icons';
import { Button, Divider, Drawer, Space, Tag, Tooltip, Typography, message } from 'antd';
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';

const { Text, Title } = Typography;

/**
 * 代码块展示组件，带复制按钮
 */
function CodeBlock({ code, label }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(code).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <div style={{ marginBottom: 16 }}>
      <Space style={{ marginBottom: 8, width: '100%', justifyContent: 'space-between' }}>
        <Text strong style={{ fontSize: 13 }}>
          {label}
        </Text>
        <Tooltip title={copied ? t('assistTask.copied') : t('assistTask.copy')}>
          <Button
            size="small"
            icon={copied ? <CheckOutlined style={{ color: '#52c41a' }} /> : <CopyOutlined />}
            onClick={handleCopy}
          >
            {t('assistTask.copy')}
          </Button>
        </Tooltip>
      </Space>
      <div
        style={{
          background: '#1e1e2e',
          borderRadius: 10,
          padding: '12px 16px',
          maxHeight: 200,
          overflowY: 'auto',
          fontFamily: '"JetBrains Mono", "Fira Code", monospace',
          fontSize: 12,
          lineHeight: 1.6,
          color: '#cdd6f4',
          whiteSpace: 'pre',
          wordBreak: 'break-all',
        }}
      >
        {code}
      </div>
    </div>
  );
}

/**
 * 辅助结果抽屉主组件
 *
 * @param {Object} props
 * @param {boolean} props.open - 是否展开
 * @param {Object|null} props.task - 任务对象（status=success 时有结果）
 * @param {function} props.onFillAll - 一键填入全部回调，参数为 task
 * @param {function} props.onClose - 关闭回调
 */
function AssistResultDrawer({ open, task, onFillAll, onClose }) {
  const { t } = useTranslation();

  if (!task) return null;

  const isAi = task.task_type === 'ai';
  const typeLabel = t(`assistTask.taskType.${task.task_type}`);
  const hasAnyRule = task.wpl_suggestion || task.oml_suggestion;

  return (
    <Drawer
      title={
        <Space>
          <Text strong>{t('assistTask.resultTitle')}</Text>
          <Tag color={isAi ? 'blue' : 'green'}>{typeLabel}</Tag>
        </Space>
      }
      placement="bottom"
      height={420}
      open={open}
      onClose={onClose}
      styles={{
        body: { padding: '16px 24px', overflowY: 'auto' },
        header: { borderBottom: '1px solid rgba(0,0,0,0.08)' },
      }}
      extra={
        <Space>
          {/* 一键填入全部区域并执行解析+转换 */}
          {task.status === 'success' && hasAnyRule && (
            <Button
              type="primary"
              size="small"
              onClick={() => onFillAll(task)}
            >
              {t('assistTask.fillAll')}
            </Button>
          )}
          <Button size="small" onClick={onClose}>
            {t('assistTask.close')}
          </Button>
        </Space>
      }
    >
      {task.status === 'success' ? (
        <>
          {/* 分析说明 */}
          {task.explanation && (
            <div style={{ marginBottom: 16 }}>
              <Text type="secondary" style={{ fontSize: 12 }}>
                {t('assistTask.explanation')}
              </Text>
              <div
                style={{
                  background: '#f9fafb',
                  border: '1px solid #e8eaed',
                  borderRadius: 10,
                  padding: '10px 14px',
                  marginTop: 6,
                  fontSize: 13,
                  lineHeight: 1.7,
                  color: '#374151',
                  whiteSpace: 'pre-wrap',
                }}
              >
                {task.explanation}
              </div>
            </div>
          )}

          {/* WPL 建议 */}
          {task.wpl_suggestion && (
            <>
              {task.explanation && <Divider style={{ margin: '12px 0' }} />}
              <CodeBlock
                code={task.wpl_suggestion}
                label={t('assistTask.wplSuggestion')}
              />
            </>
          )}

          {/* OML 建议 */}
          {task.oml_suggestion && (
            <>
              <Divider style={{ margin: '12px 0' }} />
              <CodeBlock
                code={task.oml_suggestion}
                label={t('assistTask.omlSuggestion')}
              />
            </>
          )}

          {/* 两者均无内容时兜底提示 */}
          {!hasAnyRule && (
            <Text type="secondary">{t('assistTask.noSuggestion')}</Text>
          )}
        </>
      ) : task.status === 'error' ? (
        <div>
          <Text type="danger">{task.error_message || t('assistTask.unknownError')}</Text>
        </div>
      ) : (
        <div style={{ textAlign: 'center', padding: '32px 0', color: '#999' }}>
          {t('assistTask.waitingDesc', { time: `${task.wait_seconds || 0}s` })}
        </div>
      )}
    </Drawer>
  );
}

export default AssistResultDrawer;
