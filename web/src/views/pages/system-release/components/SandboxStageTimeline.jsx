import React from 'react';
import { Card, Empty, Space, Typography } from 'antd';
import {
  CheckCircleFilled,
  CloseCircleFilled,
  ClockCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
} from '@ant-design/icons';

const { Text, Paragraph } = Typography;

const stageStatusColor = {
  success: 'green',
  failed: 'red',
  running: 'orange',
  pending: 'default',
  skipped: 'default',
  stopped: 'default',
};

const statusIcons = {
  success: <CheckCircleFilled style={{ color: '#17b26a', fontSize: 32 }} />,
  failed: <CloseCircleFilled style={{ color: '#f1554c', fontSize: 32 }} />,
  stopped: <CloseCircleFilled style={{ color: '#f97316', fontSize: 32 }} />,
  running: <LoadingOutlined style={{ color: '#275efe', fontSize: 28 }} />,
  pending: <ClockCircleOutlined style={{ color: '#98a2b3', fontSize: 28 }} />,
  skipped: <MinusCircleOutlined style={{ color: '#d0d5dd', fontSize: 28 }} />,
};

function SandboxStageTimeline({
  stages = [],
  selectedStage,
  onSelectStage,
  t,
  formatStageDuration,
  visibleStages,
  system,
}) {
  const isWfusion = system === 'wfusion';

  const stageKeyFor = (key) => {
    if (isWfusion && (key === 'start_daemon' || key === 'run_wpgen')) {
      return `${key}_wfusion`;
    }
    return key;
  };
  const filteredStages = Array.isArray(stages)
    ? stages.filter((stage) => !visibleStages || visibleStages.includes(stage.stage))
    : [];

  if (!filteredStages.length) {
    return (
      <Card>
        <Empty description={t('sandbox.noRunRecord')} />
      </Card>
    );
  }

  return (
    <Space direction="vertical" size="middle" style={{ width: '100%', minWidth: 0, overflow: 'hidden' }}>
      {filteredStages.map((stage) => {
        const stageKey = stage.stage;
        const isActive = selectedStage === stageKey;
        const diagnostics = Array.isArray(stage.diagnostics) ? stage.diagnostics : [];
        const shouldShowSummary =
          stage.summary && !['skipped', 'pending'].includes(stage.status);
        return (
          <Card
            key={stageKey}
            id={`sandbox-stage-${stageKey}`}
            hoverable
            onClick={() => onSelectStage && onSelectStage(stageKey)}
            style={{
              width: '100%',
              minWidth: 0,
              maxWidth: '100%',
              overflow: 'hidden',
              borderLeft: `4px solid ${stageStatusColor[stage.status] || '#d9d9d9'}`,
              background: isActive ? 'rgba(39,94,254,0.06)' : 'transparent',
            }}
            bodyStyle={{ padding: 16, minWidth: 0, overflow: 'hidden' }}
          >
            <Space direction="vertical" size="small" style={{ width: '100%', minWidth: 0, overflow: 'hidden' }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: 12,
                  flexWrap: 'nowrap',
                  minWidth: 0,
                }}
              >
                <Text strong style={{ flex: 1, minWidth: 0, overflowWrap: 'anywhere' }}>
                  {t(`sandbox.stage.${stageKeyFor(stageKey)}`, {
                    defaultValue: stageKey,
                  })}
                </Text>
                <div
                  style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    minWidth: 72,
                    flexShrink: 0,
                  }}
                >
                  {statusIcons[stage.status] || null}
                  {stage.duration_ms != null && (
                    <Text
                      type="secondary"
                      style={{ fontSize: 12, marginTop: 4, textAlign: 'center' }}
                    >
                      {formatStageDuration(stage.duration_ms)}
                    </Text>
                  )}
                </div>
              </div>
              {shouldShowSummary && (
                <Paragraph style={{ marginBottom: 4, overflowWrap: 'anywhere', whiteSpace: 'pre-line' }}>
                  {stage.summary}
                </Paragraph>
              )}
              {diagnostics.length > 0 && (
                <div style={{ minWidth: 0 }}>
                  <Text type="secondary">{t('sandbox.stageDiagnostics')}</Text>
                  <ul style={{ paddingLeft: 18, marginBottom: 0, marginTop: 4, minWidth: 0 }}>
                    {diagnostics.map((item, index) => (
                      <li key={`${stageKey}-diag-${index}`}>
                        <Text style={{ overflowWrap: 'anywhere' }}>{item.suggestion}</Text>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </Space>
          </Card>
        );
      })}
      <Text type="secondary">{t('sandbox.stageClickHint')}</Text>
    </Space>
  );
}

export default SandboxStageTimeline;
