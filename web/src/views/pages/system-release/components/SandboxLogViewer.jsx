import React from 'react';
import { Card, Button, Space, Spin, Typography } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';

const { Text, Paragraph } = Typography;

function SandboxLogViewer({
  selectedStageInfo,
  stageLog,
  stageLogLoading,
  stageLogMeta,
  onRefresh,
  t,
  formatStageDuration,
  formatDisplayTime,
  cardStyle,
  cardId,
}) {
  const stageName = selectedStageInfo
    ? t(`sandbox.stage.${selectedStageInfo.stage}`, { defaultValue: selectedStageInfo.stage })
    : t('sandbox.logViewer');
  const statusText = selectedStageInfo
    ? t(`sandbox.stageStatusLabel.${selectedStageInfo.status}`, {
        defaultValue: selectedStageInfo.status,
      })
    : '';

  return (
    <Card
      id={cardId}
      title={t('sandbox.logViewer')}
      style={{ width: '100%', ...cardStyle }}
      extra={
        <Button
          icon={<ReloadOutlined />}
          onClick={() => selectedStageInfo && onRefresh && onRefresh(selectedStageInfo.stage)}
          disabled={!selectedStageInfo}
        >
          {t('sandbox.refreshLog')}
        </Button>
      }
    >
      <Space direction="vertical" size="small" style={{ width: '100%' }}>
        <Text strong>{stageName}</Text>
        {selectedStageInfo && (
          <Text type="secondary">
            {statusText}
            {selectedStageInfo.duration_ms != null &&
              ` · ${t('sandbox.stageDurationLabel')}: ${formatStageDuration(
                selectedStageInfo.duration_ms,
              )}`}
          </Text>
        )}
        {selectedStageInfo?.summary && (
          <Paragraph style={{ marginBottom: 0 }}>{selectedStageInfo.summary}</Paragraph>
        )}
        <div
          style={{
            background: '#0b1020',
            padding: 12,
            borderRadius: 8,
            minHeight: 420,
            maxHeight: 640,
            overflowY: 'auto',
            color: '#f1f1f1',
            fontFamily: 'Menlo, Consolas, monospace',
            fontSize: 12,
          }}
        >
          {stageLogLoading ? (
            <Space align="center" style={{ width: '100%', justifyContent: 'center' }}>
              <Spin />
            </Space>
          ) : (
            <pre style={{ whiteSpace: 'pre-wrap', margin: 0 }}>
              {stageLog || t('sandbox.noLogContent')}
            </pre>
          )}
        </div>
        <Space size="small">
          <Text type="secondary">{t('sandbox.logPath')}</Text>
          <Text code>{stageLogMeta?.logPath || '-'}</Text>
        </Space>
        <Text type="secondary">
          {stageLogMeta?.fetchedAt
            ? `${t('sandbox.logUpdatedAt')}: ${formatDisplayTime(stageLogMeta.fetchedAt)}`
            : t('sandbox.logNotFetched')}
        </Text>
        <Text type="secondary">{t('sandbox.logHint')}</Text>
      </Space>
    </Card>
  );
}

export default SandboxLogViewer;
