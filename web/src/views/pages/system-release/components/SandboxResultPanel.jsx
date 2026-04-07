import React, { useMemo } from 'react';
import dayjs from 'dayjs';
import { Card, Divider, Space, Tag, Typography } from 'antd';

const { Text, Paragraph } = Typography;

function SandboxResultPanel({
  runData,
  conclusion,
  t,
  formatStageDuration,
  formatDisplayTime,
  failureSummary,
  cardStyle,
}) {
  const status = runData?.status || 'queued';
  const statusColor = {
    queued: 'default',
    running: 'blue',
    success: 'green',
    failed: 'red',
    stopped: 'default',
  }[status];
  const totalDurationMs = useMemo(() => {
    if (!runData?.started_at || !runData?.ended_at) {
      return null;
    }
    const start = dayjs(runData.started_at);
    const end = dayjs(runData.ended_at);
    return Math.max(0, end.diff(start, 'millisecond'));
  }, [runData?.started_at, runData?.ended_at]);

  const failedStageLabel = conclusion?.failed_stage
    ? t(`sandbox.stage.${conclusion.failed_stage}`, {
        defaultValue: conclusion.failed_stage,
      })
    : '-';

  const executionMessages = useMemo(() => {
    if (!runData) {
      return [t('sandbox.executionResultIdle')];
    }
    if (runData.status === 'success') {
      const total =
        conclusion?.input_count ?? runData?.options?.sample_count ?? 0;
      return [t('sandbox.executionResultSuccess', { count: total })];
    }
    if (runData.status === 'failed') {
      const messages = [];
      if (failureSummary) {
        messages.push(failureSummary);
      }
      if (Array.isArray(conclusion?.top_suggestions) && conclusion.top_suggestions.length > 0) {
        messages.push(...conclusion.top_suggestions);
      }
      if (messages.length > 0) {
        return messages;
      }
      return [t('sandbox.executionResultFailed')];
    }
    if (runData.status === 'running' || runData.status === 'queued') {
      return [t('sandbox.executionResultPending')];
    }
    if (runData.status === 'stopped') {
      return [t('sandbox.executionResultStopped')];
    }
    return [t('sandbox.executionResultFailed')];
  }, [conclusion?.input_count, conclusion?.top_suggestions, failureSummary, runData, t]);

  return (
    <Card title={t('sandbox.resultOverview')} style={{ width: '100%', ...cardStyle }}>
      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
        <Space align="center" size="small">
          <Text type="secondary">{t('sandbox.statusLabelTitle')}</Text>
          <Tag color={statusColor || 'default'}>
            {t(`sandbox.statusLabel.${status}`, { defaultValue: status })}
          </Tag>
        </Space>
        <div>
          <Text type="secondary">{t('sandbox.taskId')}</Text>
          <Paragraph style={{ marginBottom: 0 }}>{runData?.task_id || '-'}</Paragraph>
        </div>
        <Space size="large" align="start" wrap>
          <div>
            <Text type="secondary">{t('sandbox.startTime')}</Text>
            <Paragraph style={{ marginBottom: 0 }}>{formatDisplayTime(runData?.started_at)}</Paragraph>
          </div>
          <div>
            <Text type="secondary">{t('sandbox.endTime')}</Text>
            <Paragraph style={{ marginBottom: 0 }}>{formatDisplayTime(runData?.ended_at)}</Paragraph>
          </div>
        </Space>
        <div>
          <Text type="secondary">{t('sandbox.totalDuration')}</Text>
          <Paragraph style={{ marginBottom: 0 }}>
            {totalDurationMs != null ? formatStageDuration(totalDurationMs) : '--'}
          </Paragraph>
        </div>
        <div>
          <Text type="secondary">{t('sandbox.failedStage')}</Text>
          <Paragraph style={{ marginBottom: 0 }}>{failedStageLabel}</Paragraph>
        </div>
        <div>
          <Text type="secondary">{t('sandbox.workspacePath')}</Text>
          <Paragraph style={{ marginBottom: 0, fontFamily: 'monospace' }}>
            {runData?.workspace_path || '-'}
          </Paragraph>
        </div>
        <Divider />
        <div>
          <Text strong>{t('sandbox.executionResult')}</Text>
          <Space direction="vertical" size="small" style={{ width: '100%', marginTop: 8 }}>
            {executionMessages.map((msg, index) => (
              <Paragraph key={`execution-msg-${index}`} style={{ marginBottom: 0 }}>
                {msg}
              </Paragraph>
            ))}
          </Space>
        </div>
      </Space>
    </Card>
  );
}

export default SandboxResultPanel;
