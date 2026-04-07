import React, { useEffect, useMemo, useState } from 'react';
import dayjs from 'dayjs';
import { Card, Empty, Pagination, Skeleton, Space, Tag, Typography } from 'antd';

const { Text } = Typography;

const STATUS_COLOR = {
  success: 'green',
  failed: 'red',
  running: 'blue',
  queued: 'default',
  stopped: 'default',
};

function SandboxHistoryList({
  history = [],
  loading,
  activeTaskId,
  onSelect,
  t,
  totalCount,
  cardStyle,
  page,
  pageSize,
  onPageChange,
  maxTotal,
}) {
  const [internalPage, setInternalPage] = useState(1);
  const effectivePage = page ?? internalPage;
  const effectivePageSize = pageSize ?? 5;

  useEffect(() => {
    if (page !== undefined) {
      setInternalPage(page);
    }
  }, [page]);

  const displayList = useMemo(() => {
    const start = (effectivePage - 1) * effectivePageSize;
    const end = start + effectivePageSize;
    return history.slice(start, end);
  }, [history, effectivePage, effectivePageSize]);

  const handlePageChange = (nextPage) => {
    if (onPageChange) {
      onPageChange(nextPage);
    } else {
      setInternalPage(nextPage);
    }
  };

  if (loading) {
    return (
      <Card title={t('sandbox.history')} style={{ height: '100%', ...cardStyle }}>
        <Skeleton active paragraph={{ rows: 4 }} />
      </Card>
    );
  }

  if (!history.length) {
    return (
      <Card title={t('sandbox.history')} style={{ height: '100%', ...cardStyle }}>
        <Empty description={t('sandbox.historyEmpty')} />
      </Card>
    );
  }

  const computedTotal = totalCount ?? history.length;
  const cappedTotal =
    maxTotal != null ? Math.min(computedTotal, maxTotal) : computedTotal;
  const paginationTotal = Math.max(cappedTotal, history.length);

  return (
    <Card
      title={t('sandbox.history')}
      style={{ height: '100%', ...cardStyle }}
      bodyStyle={{ paddingTop: 12 }}
    >
      <Text type="secondary" style={{ display: 'block', marginBottom: 8 }}>
        {t('sandbox.historyTotal', { total: totalCount ?? history.length })}
      </Text>
      <Space direction="vertical" size="small" style={{ width: '100%' }}>
        {displayList.map((item) => {
          const startText = item.started_at ? dayjs(item.started_at).format('MM-DD HH:mm:ss') : '--';
          const duration =
            item.duration_ms != null ? `${(item.duration_ms / 1000).toFixed(1)}s` : '--';
          const isActive = item.task_id === activeTaskId;
          const statusColor = STATUS_COLOR[item.status] || 'default';
          return (
            <div
              key={item.task_id}
              onClick={() => onSelect && onSelect(item.task_id)}
              style={{
                border: `1px solid ${isActive ? '#275efe' : '#f0f0f0'}`,
                borderRadius: 10,
                padding: 12,
                cursor: 'pointer',
                background: isActive ? 'rgba(39,94,254,0.08)' : '#fff',
              }}
            >
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                <Space align="center" size="small">
                  <Tag color={statusColor}>
                    {t(`sandbox.statusLabel.${item.status}`, { defaultValue: item.status })}
                  </Tag>
                  <Text strong>{startText}</Text>
                </Space>
                <Space size="small" wrap>
                  <Text type="secondary">{t('sandbox.historySampleCount')}</Text>
                  <Text>{item.sample_count}</Text>
                  <Text type="secondary">·</Text>
                  <Text type="secondary">{t('sandbox.stageDurationLabel')}</Text>
                  <Text>{duration}</Text>
                </Space>
              </Space>
            </div>
          );
        })}
      </Space>
      <div
        style={{
          marginTop: 12,
          display: 'flex',
          justifyContent: 'flex-end',
        }}
      >
        <Pagination
          size="small"
          current={effectivePage}
          pageSize={effectivePageSize}
          total={paginationTotal}
          onChange={handlePageChange}
          showSizeChanger={false}
        />
      </div>
    </Card>
  );
}

export default SandboxHistoryList;
