import React, { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Alert, Button, Card, Col, Descriptions, Modal, Progress, Row, Select, Spin, Statistic, Table, Tag, Tooltip, message } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';
import { fetchAssetRuns, fetchGeoRuns, fetchIntelRuns, fetchKnowledgeSyncStatus, fetchMmdbFiles, startGeoImport } from '@/services/knowledge_sync';

const HISTORY_MODAL_LAYOUT = {
  asset: { width: 1120 },
  geo: { width: 1300 },
  intel: { width: 1100 },
};

const formatTime = (value, locale) => {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString(locale);
};

const durationSeconds = (run, now) => {
  if (!run?.startedAt) return '—';
  const start = new Date(run.startedAt).getTime();
  const end = run.finishedAt ? new Date(run.finishedAt).getTime() : now;
  return Number.isFinite(start) && Number.isFinite(end) ? Math.max(0, Math.floor((end - start) / 1000)) : '—';
};

function KnowledgeSyncPage() {
  const { t, i18n } = useTranslation();
  const [asset, setAsset] = useState(null);
  const [geo, setGeo] = useState(null);
  const [intel, setIntel] = useState(null);
  const [files, setFiles] = useState([]);
  const [selectedFile, setSelectedFile] = useState(null);
  const [startingImport, setStartingImport] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [historyKind, setHistoryKind] = useState(null);
  const [historyPage, setHistoryPage] = useState(1);
  const [historyPageSize, setHistoryPageSize] = useState(10);
  const [history, setHistory] = useState({ items: [], total: 0 });
  const [historyLoading, setHistoryLoading] = useState(false);

  const loadStatus = useCallback(async (showError = true) => {
    try {
      const result = await fetchKnowledgeSyncStatus();
      setAsset(result.asset);
      setGeo(result.geo);
      setIntel(result.intel);
    } catch (error) {
      if (showError) {
        message.error(
          t('knowledgeSync.loadFailed', { message: error?.message || t('common.unknownError') }),
        );
      }
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, [t]);

  const loadFiles = useCallback(async () => {
    try {
      const available = await fetchMmdbFiles();
      setFiles(available);
      setSelectedFile((current) => available.some((file) => file.name === current) ? current : null);
    } catch (error) {
      message.error(t('knowledgeSync.geoFilesFailed', { message: error?.message }));
    }
  }, [t]);

  useEffect(() => {
    loadStatus();
    const timer = window.setInterval(() => loadStatus(false), geo?.latestRun?.status === 'running' || intel?.latestRun?.status === 'running' ? 5000 : 30000);
    return () => window.clearInterval(timer);
  }, [loadStatus, geo?.latestRun?.status, intel?.latestRun?.status]);

  useEffect(() => {
    if (geo?.latestRun?.status !== 'running') return undefined;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [geo?.latestRun?.status]);

  useEffect(() => {
    if (!geo?.configured || geo?.error) {
      setFiles([]);
      setSelectedFile(null);
      return;
    }
    loadFiles();
  }, [geo?.configured, geo?.error, loadFiles]);

  useEffect(() => {
    if (!historyKind) return undefined;
    let active = true;
    setHistoryLoading(true);
    const fetchRuns = historyKind === 'asset' ? fetchAssetRuns : historyKind === 'geo' ? fetchGeoRuns : fetchIntelRuns;
    fetchRuns(historyPage, historyPageSize)
      .then((result) => { if (active) setHistory(result); })
      .catch((error) => {
        if (active) message.error(t('knowledgeSync.historyFailed', { message: error?.message }));
      })
      .finally(() => { if (active) setHistoryLoading(false); });
    return () => { active = false; };
  }, [historyKind, historyPage, historyPageSize, t]);

  const openHistory = (kind) => {
    setHistory({ items: [], total: 0 });
    setHistoryPage(1);
    setHistoryPageSize(10);
    setHistoryKind(kind);
  };

  const refresh = () => {
    setRefreshing(true);
    loadStatus();
    if (geo?.configured && !geo?.error) loadFiles();
  };

  const importMmdb = async () => {
    if (!selectedFile) return;
    setStartingImport(true);
    try {
      const run = await startGeoImport(selectedFile);
      setGeo((current) => ({ ...current, latestRun: run }));
      message.success(t('knowledgeSync.geoImportStarted'));
    } catch (error) {
      message.error(t('knowledgeSync.geoImportFailed', {
        message: error?.response?.data?.error?.message || error?.message || t('common.unknownError'),
      }));
    } finally {
      setStartingImport(false);
    }
  };

  const statusColor = !asset?.configured
    ? 'default'
    : asset?.error || asset?.latestRunStatus === 'failed'
      ? 'error'
      : asset?.taskActive && asset?.initialized
        ? 'success'
        : 'warning';
  const statusText = !asset?.configured
    ? t('knowledgeSync.notConfigured')
    : asset?.error
      ? asset?.currentRowCount != null
        ? t('knowledgeSync.taskUnavailable')
        : t('knowledgeSync.initializationFailed')
      : asset?.taskActive
        ? t('knowledgeSync.taskActive')
        : t('knowledgeSync.taskUnavailable');
  const intelSchedule = intel?.intervalSeconds == null ? '—'
    : intel.intervalSeconds % 3600 === 0
      ? t('knowledgeSync.everyHours', { count: intel.intervalSeconds / 3600 })
      : intel.intervalSeconds % 60 === 0
        ? t('knowledgeSync.everyMinutes', { count: intel.intervalSeconds / 60 })
        : t('knowledgeSync.everySeconds', { count: intel.intervalSeconds });

  const assetHistoryColumns = [
    { title: t('knowledgeSync.historyId'), dataIndex: 'id', width: 65 },
    { title: t('knowledgeSync.lastRun'), dataIndex: 'status', width: 90,
      render: (value) => t(`knowledgeSync.runStatus.${value}`, { defaultValue: value }) },
    ...['assetCount', 'terminalCount', 'networkCount', 'totalAssetCount'].map((field) => ({
      title: t(`knowledgeSync.${field === 'assetCount' ? 'hostCount' : field}`), dataIndex: field, width: 100,
      render: (value) => value?.toLocaleString() ?? '—',
    })),
    { title: t('knowledgeSync.historyStart'), dataIndex: 'startedAt', width: 155,
      render: (value) => formatTime(value, i18n.language) },
    { title: t('knowledgeSync.historyEnd'), dataIndex: 'finishedAt', width: 155,
      render: (value) => formatTime(value, i18n.language) },
    { title: t('knowledgeSync.historyMessage'), dataIndex: 'message', ellipsis: true,
      render: (value) => <Tooltip title={value}><span>{value || '—'}</span></Tooltip> },
  ];
  const geoHistoryColumns = [
    { title: t('knowledgeSync.historyId'), dataIndex: 'id', width: 62 },
    { title: t('knowledgeSync.geoFile'), dataIndex: 'fileName', width: 190, ellipsis: true,
      render: (value) => <Tooltip title={value}><span>{value || '—'}</span></Tooltip> },
    { title: t('knowledgeSync.geoHash'), dataIndex: 'fileHash', width: 120,
      render: (value) => value ? <Tooltip title={value}><span>{value.slice(0, 12)}…</span></Tooltip> : '—' },
    { title: t('knowledgeSync.lastRun'), dataIndex: 'status', width: 90,
      render: (value) => value === 'running' ? t('knowledgeSync.geoImporting')
        : t(`knowledgeSync.runStatus.${value}`, { defaultValue: value }) },
    { title: t('knowledgeSync.geoLastImportRows'), dataIndex: 'rowCount', width: 105,
      render: (value) => value?.toLocaleString() ?? '—' },
    { title: t('knowledgeSync.geoHistoryIpv4'), dataIndex: 'sourceRowCount', width: 105,
      render: (value) => value?.toLocaleString() ?? '—' },
    { title: t('knowledgeSync.geoHistoryIpv6'), dataIndex: 'ipv6SourceRowCount', width: 105,
      render: (value) => value?.toLocaleString() ?? '—' },
    { title: t('knowledgeSync.geoCopyProgress'), dataIndex: 'progressPercent', width: 90,
      render: (value) => value == null ? '—' : `${value}%` },
    { title: t('knowledgeSync.historyStart'), dataIndex: 'startedAt', width: 160,
      render: (value) => formatTime(value, i18n.language) },
    { title: t('knowledgeSync.historyDurationSeconds'), width: 95,
      render: (_, run) => durationSeconds(run, now) },
    { title: t('knowledgeSync.historyMessage'), dataIndex: 'error', ellipsis: true,
      render: (value) => <Tooltip title={value}><span>{value || '—'}</span></Tooltip> },
  ];
  const intelHistoryColumns = [
    { title: t('knowledgeSync.historyId'), dataIndex: 'id', width: 65 },
    { title: t('knowledgeSync.lastRun'), dataIndex: 'status', width: 90,
      render: (value) => t(`knowledgeSync.runStatus.${value}`, { defaultValue: value }) },
    { title: t('knowledgeSync.historyRead'), dataIndex: 'readCount', width: 85 },
    { title: t('knowledgeSync.historyExpired'), dataIndex: 'expiredCount', width: 85 },
    { title: t('knowledgeSync.historyInvalid'), dataIndex: 'invalidCount', width: 85 },
    { title: t('knowledgeSync.historyIp'), dataIndex: 'ipCount', width: 85 },
    { title: t('knowledgeSync.historyDomain'), dataIndex: 'domainCount', width: 85 },
    { title: t('knowledgeSync.historyStart'), dataIndex: 'startedAt', width: 150,
      render: (value) => formatTime(value, i18n.language) },
    { title: t('knowledgeSync.historyEnd'), dataIndex: 'finishedAt', width: 150,
      render: (value) => formatTime(value, i18n.language) },
    { title: t('knowledgeSync.historyMessage'), dataIndex: 'error', ellipsis: true,
      render: (value) => <Tooltip title={value}><span>{value || '—'}</span></Tooltip> },
  ];

  return (
    <section className="page-panels knowledge-sync-page">
      <article className="panel is-visible">
        <header className="panel-header">
          <h2>{t('knowledgeSync.title')}</h2>
          <Button icon={<ReloadOutlined />} loading={refreshing} onClick={refresh}>
            {t('knowledgeSync.refresh')}
          </Button>
        </header>
        <section className="panel-body">
          <Spin spinning={loading}>
            {asset && (
              <Card
                title={t('knowledgeSync.assetTitle')}
                extra={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <Button type="link" onClick={() => openHistory('asset')} disabled={!asset.configured}>
                    {t('knowledgeSync.history')}
                  </Button>
                  <Tag color={statusColor}>{statusText}</Tag>
                </div>}
              >
                {asset.error && (
                  <Alert
                    type="error"
                    showIcon
                    message={asset.error}
                    style={{ marginBottom: 16 }}
                  />
                )}
                {!asset.configured && (
                  <Alert
                    type="info"
                    showIcon
                    message={t('knowledgeSync.environmentNotConfigured')}
                    style={{ marginBottom: 16 }}
                  />
                )}
                <Row gutter={[16, 16]} style={{ marginBottom: 16 }}>
                  <Col xs={12} md={6}>
                    <Statistic
                      title={t('knowledgeSync.hostCount')}
                      value={asset.latestAssetCount ?? 0}
                    />
                  </Col>
                  <Col xs={12} md={6}>
                    <Statistic
                      title={t('knowledgeSync.terminalCount')}
                      value={asset.latestTerminalCount ?? 0}
                    />
                  </Col>
                  <Col xs={12} md={6}>
                    <Statistic
                      title={t('knowledgeSync.networkCount')}
                      value={asset.latestNetworkCount ?? 0}
                    />
                  </Col>
                  <Col xs={12} md={6}>
                    <Statistic
                      title={t('knowledgeSync.totalAssetCount')}
                      value={asset.latestAssetCount == null || asset.latestTerminalCount == null || asset.latestNetworkCount == null
                        ? 0 : asset.latestAssetCount + asset.latestTerminalCount + asset.latestNetworkCount}
                    />
                  </Col>
                </Row>
                <Descriptions column={{ xs: 1, sm: 2 }} size="small">
                  <Descriptions.Item label={t('knowledgeSync.schedule')}>
                    {asset.taskSchedule || '—'}
                  </Descriptions.Item>
                  <Descriptions.Item label={t('knowledgeSync.lastDataTime')}>
                    {formatTime(asset.latestDataAt, i18n.language)}
                  </Descriptions.Item>
                </Descriptions>
                {asset.latestRunMessage && asset.latestRunStatus === 'failed' && (
                  <Alert
                    type="error"
                    showIcon
                    message={t('knowledgeSync.lastRunError')}
                    description={asset.latestRunMessage}
                    style={{ marginTop: 16 }}
                  />
                )}
              </Card>
            )}
            {geo && (
              <Card
                title={<div className="knowledge-sync-geo-title">
                  <span>{t('knowledgeSync.geoTitle')}</span>
                  {geo.configured && !geo.error && <div className="knowledge-sync-geo-import">
                    <Select
                      placeholder={t('knowledgeSync.geoSelectFile')}
                      value={selectedFile}
                      onChange={setSelectedFile}
                      options={files.map((file) => ({ label: `${file.name} (${(file.size / 1024 / 1024).toFixed(1)} MB)`, value: file.name }))}
                      disabled={geo.latestRun?.status === 'running'}
                      style={{ minWidth: 260 }}
                    />
                    <Button type="primary" onClick={importMmdb} loading={startingImport}
                      disabled={!selectedFile || geo.latestRun?.status === 'running'}>
                      {t('knowledgeSync.geoImport')}
                    </Button>
                  </div>}
                </div>}
                style={{ marginTop: 16 }}
                extra={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <Button type="link" onClick={() => openHistory('geo')} disabled={!geo.configured}>
                    {t('knowledgeSync.history')}
                  </Button>
                  <Tag color={geo.error ? 'error' : !geo.configured ? 'default' : geo.tableExists ? 'success' : 'warning'}>
                    {!geo.configured ? t('knowledgeSync.notConfigured') : geo.error ? t('knowledgeSync.initializationFailed')
                      : geo.tableExists ? t('knowledgeSync.geoReady') : t('knowledgeSync.geoMissing')}
                  </Tag>
                </div>}
              >
                {geo.error && <Alert type="error" showIcon message={geo.error} style={{ marginBottom: 16 }} />}
                {!geo.configured && <Alert type="info" showIcon message={t('knowledgeSync.geoNotConfigured')} style={{ marginBottom: 16 }} />}
                {geo.tableExists && !geo.indexReady && <Alert type="warning" showIcon message={t('knowledgeSync.geoIndexMissing')} style={{ marginBottom: 16 }} />}
                {(geo.sampleLimit ?? geo.latestRun?.rowLimit) != null && <Alert type="info" showIcon message={t('knowledgeSync.geoSampleNotice', { count: geo.sampleLimit ?? geo.latestRun.rowLimit })} style={{ marginBottom: 16 }} />}
                <Row gutter={[16, 16]} style={{ marginBottom: 16 }}>
                  <Col xs={12} md={6}>
                    <Statistic title={t('knowledgeSync.geoSourceRows')}
                      value={geo.latestRun?.status === 'running'
                        ? geo.latestRun.sourceRowCount ?? '—'
                        : geo.sourceRowCount ?? '—'} />
                  </Col>
                  <Col xs={12} md={6}>
                    <Statistic title={t('knowledgeSync.geoIpv6SourceRows')}
                      value={geo.latestRun?.status === 'running'
                        ? geo.latestRun.ipv6SourceRowCount ?? 0
                        : geo.ipv6SourceRowCount ?? '—'} />
                  </Col>
                  <Col xs={12} md={6}>
                    <Statistic title={t('knowledgeSync.geoImportedRows')}
                      value={geo.latestRun?.status === 'running' ? '—' : geo.currentRowCount ?? 0} />
                  </Col>
                  <Col xs={12} md={6}>
                    <Statistic title={t('knowledgeSync.geoDurationSeconds')}
                      value={durationSeconds(geo.latestRun, now)} />
                  </Col>
                </Row>
                <Descriptions className="knowledge-sync-geo-details" column={{ xs: 1, md: 2, lg: 4 }} size="small">
                  <Descriptions.Item label={t('knowledgeSync.geoVersion')}>
                    {geo.fileName?.replace(/\.mmdb$/i, '') || t('knowledgeSync.versionUnknown')}
                  </Descriptions.Item>
                  <Descriptions.Item label={t('knowledgeSync.geoHash')}>
                    <span className="knowledge-sync-hash" title={geo.fileHash || '—'}>{geo.fileHash || '—'}</span>
                  </Descriptions.Item>
                  <Descriptions.Item label={t('knowledgeSync.lastDataTime')}>
                    {formatTime(geo.latestDataAt, i18n.language)}
                  </Descriptions.Item>
                  <Descriptions.Item label={t('knowledgeSync.geoStage')}>
                    {geo.latestRun ? t(`knowledgeSync.geoStages.${geo.latestRun.stage}`, { defaultValue: geo.latestRun.stage }) : '—'}
                  </Descriptions.Item>
                  {geo.latestRun?.status === 'running' && <>
                    <Descriptions.Item label={t('knowledgeSync.pendingGeoVersion')}>
                      {geo.latestRun.fileName?.replace(/\.mmdb$/i, '') || '—'}
                    </Descriptions.Item>
                    <Descriptions.Item label={t('knowledgeSync.pendingGeoHash')}>
                      <span className="knowledge-sync-hash" title={geo.latestRun.fileHash || '—'}>{geo.latestRun.fileHash || '—'}</span>
                    </Descriptions.Item>
                  </>}
                </Descriptions>
                {geo.latestRun?.error && <Alert type="error" showIcon message={geo.latestRun.error} style={{ marginTop: 16 }} />}
                {geo.latestRun?.status === 'running' && (
                  <div style={{ marginTop: 16 }}>
                    <Tag color="processing">{t('knowledgeSync.geoImporting')}</Tag>
                    <span>{t('knowledgeSync.geoProgressRows', { count: geo.latestRun.progressRows })}</span>
                    <Progress percent={geo.latestRun.progressPercent ?? 0} status="active"
                      format={() => geo.latestRun.progressPercent == null
                        ? t(`knowledgeSync.geoStages.${geo.latestRun.stage}`, { defaultValue: geo.latestRun.stage })
                        : `${geo.latestRun.progressPercent}%`}
                      style={{ marginTop: 8 }} />
                  </div>
                )}
              </Card>
            )}
            {intel && (
              <Card title={t('knowledgeSync.intelTitle')} style={{ marginTop: 16 }}
                extra={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <Button type="link" onClick={() => openHistory('intel')} disabled={!intel.configured}>
                    {t('knowledgeSync.history')}
                  </Button>
                  <Tag color={!intel.configured ? 'default' : intel.error || !intel.intervalSeconds ? 'warning' : 'success'}>
                    {!intel.configured ? t('knowledgeSync.notConfigured')
                      : intel.error || !intel.intervalSeconds ? t('knowledgeSync.taskUnavailable')
                        : t('knowledgeSync.taskActive')}
                  </Tag>
                </div>}>
                {intel.error && <Alert type="error" showIcon message={intel.error} style={{ marginBottom: 16 }} />}
                {!intel.configured && <Alert type="info" showIcon message={t('knowledgeSync.intelNotConfigured')} style={{ marginBottom: 16 }} />}
                <Row gutter={[16, 16]} style={{ marginBottom: 16 }}>
                  <Col xs={12} md={8}><Statistic title={t('knowledgeSync.intelIp')}
                    value={intel.latestSuccessRun?.ipCount ?? 0} /></Col>
                  <Col xs={12} md={8}><Statistic title={t('knowledgeSync.intelDomain')}
                    value={intel.latestSuccessRun?.domainCount ?? 0} /></Col>
                  <Col xs={12} md={8}><Statistic title={t('knowledgeSync.intelTotal')}
                    value={intel.latestSuccessRun
                      ? intel.latestSuccessRun.ipCount + intel.latestSuccessRun.domainCount : 0} /></Col>
                </Row>
                <Descriptions column={{ xs: 1, sm: 2 }} size="small">
                  <Descriptions.Item label={t('knowledgeSync.schedule')}>
                    {intelSchedule}
                  </Descriptions.Item>
                  <Descriptions.Item label={t('knowledgeSync.lastDataTime')}>
                    {formatTime(intel.latestSuccessRun?.finishedAt, i18n.language)}
                  </Descriptions.Item>
                </Descriptions>
                {intel.latestRun?.error && <Alert type="error" showIcon message={intel.latestRun.error} style={{ marginTop: 16 }} />}
              </Card>
            )}
          </Spin>
        </section>
      </article>
      <Modal
        title={historyKind === 'asset' ? t('knowledgeSync.assetHistory') : historyKind === 'geo' ? t('knowledgeSync.geoHistory') : t('knowledgeSync.intelHistory')}
        open={Boolean(historyKind)}
        onCancel={() => setHistoryKind(null)}
        maskClosable
        footer={null}
        width={HISTORY_MODAL_LAYOUT[historyKind]?.width ?? HISTORY_MODAL_LAYOUT.asset.width}
        style={{ top: 48, maxWidth: 'calc(100vw - 48px)' }}
        styles={{ body: { overflow: 'hidden' } }}
      >
        <Table
          className="knowledge-sync-history-table"
          rowKey="id"
          columns={historyKind === 'asset' ? assetHistoryColumns : historyKind === 'geo' ? geoHistoryColumns : intelHistoryColumns}
          dataSource={history.items}
          loading={historyLoading}
          tableLayout="fixed"
          scroll={{ y: 'min(520px, calc(100vh - 320px))' }}
          pagination={{
            current: historyPage,
            pageSize: historyPageSize,
            total: history.total,
            showSizeChanger: true,
            pageSizeOptions: [10, 20, 50],
            showTotal: (total) => t('knowledgeSync.historyTotal', { count: total }),
            onChange: (page, pageSize) => {
              setHistoryPage(pageSize === historyPageSize ? page : 1);
              setHistoryPageSize(pageSize);
            },
          }}
        />
      </Modal>
    </section>
  );
}

export default KnowledgeSyncPage;
