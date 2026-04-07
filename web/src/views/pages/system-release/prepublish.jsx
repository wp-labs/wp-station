import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import dayjs from 'dayjs';
import { useNavigate, useParams } from 'react-router-dom';
import { Alert, Button, Card, Col, Form, InputNumber, Row, Space, Tag, Typography, message } from 'antd';
import { ArrowLeftOutlined, PauseCircleOutlined, PlayCircleOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { fetchReleaseDetail } from '@/services/release';
import {
  createSandboxRun,
  fetchLatestSandboxRun,
  fetchSandboxHistory,
  fetchSandboxRun,
  fetchSandboxStageLogs,
  stopSandboxRun,
} from '@/services/sandbox';
import SandboxStageTimeline from './components/SandboxStageTimeline';
import SandboxResultPanel from './components/SandboxResultPanel';
import SandboxLogViewer from './components/SandboxLogViewer';
import SandboxHistoryList from './components/SandboxHistoryList';

const { Title, Text, Paragraph } = Typography;

const DEFAULT_RUNTIME_SECONDS = 5;

const DEFAULT_OPTIONS = {
  sample_count: 10,
  startup_timeout_ms: 30000,
  runtime_collect_ms: DEFAULT_RUNTIME_SECONDS * 1000,
  wpgen_timeout_ms: 60000,
  keep_workspace: false,
};

const pollingStatuses = new Set(['queued', 'running']);

const HISTORY_PAGE_SIZE = 5;
const HISTORY_FETCH_LIMIT = 100;
const MAX_HISTORY_PAGE = Math.ceil(HISTORY_FETCH_LIMIT / HISTORY_PAGE_SIZE);

const formatDisplayTime = (value) => (value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-');

const formatStageDuration = (ms) => {
  if (ms == null) return '--';
  const raw = `${ms}ms`;
  if (ms >= 60000) {
    return `${(ms / 60000).toFixed(1)}m (${raw})`;
  }
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(1)}s (${raw})`;
  }
  return raw;
};

const STATUS_COLOR = {
  queued: 'default',
  running: 'blue',
  success: 'green',
  failed: 'red',
  stopped: 'default',
  not_started: 'default',
};

const VISIBLE_STAGE_KEYS = [
  'prepare_workspace',
  'preflight_check',
  'start_daemon',
  'run_wpgen',
  'analyse_runtime_output',
];

const EMPTY_STAGES = [];
const EMPTY_STAGE_LOG_META = {
  stage: null,
  logPath: '',
  fetchedAt: null,
};

function PrepublishPage() {
  const { id } = useParams();
  const releaseId = Number(id);
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [form] = Form.useForm();
  const [releaseInfo, setReleaseInfo] = useState(null);
  const [runData, setRunData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [polling, setPolling] = useState(false);
  const [queuePosition, setQueuePosition] = useState(null);
  const [selectedStage, setSelectedStage] = useState(null);
  const [stageLog, setStageLog] = useState('');
  const [stageLogMeta, setStageLogMeta] = useState(EMPTY_STAGE_LOG_META);
  const [stageLogLoading, setStageLogLoading] = useState(false);
  const [historyData, setHistoryData] = useState({ total: 0, items: [] });
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyPage, setHistoryPage] = useState(1);
  const autoScrollTaskRef = useRef(null);
  const logAutoRefreshRef = useRef(null);
  const lastTaskIdRef = useRef(null);
  const [autoFollowStage, setAutoFollowStage] = useState(false);
  const stageLogCardRef = useRef(null);

  const stages = runData?.stages ?? EMPTY_STAGES;
  const filteredStages = useMemo(
    () => stages.filter((stage) => VISIBLE_STAGE_KEYS.includes(stage.stage)),
    [stages],
  );
  const conclusion = runData?.conclusion;
  const selectedStageInfo = stages.find((item) => item.stage === selectedStage);
  const failureStageInfo = stages.find((stage) => stage.status === 'failed');
  const failureSummary = failureStageInfo?.summary;
  const status = runData?.status || 'queued';
  const baselineVersion =
    releaseInfo?.previous_version ||
    releaseInfo?.baseline_version ||
    releaseInfo?.pipeline ||
    t('sandbox.baselineFallback');
  const currentVersion = releaseInfo?.version || '-';
  const statusTag = (
    <Tag color={STATUS_COLOR[status] || 'default'}>
      {t(`sandbox.statusLabel.${status}`, { defaultValue: status })}
    </Tag>
  );
  const queueAlertNeeded = (queuePosition ?? 0) > 0;
  const historyItems = historyData.items || [];
  const canStop = runData?.status && ['queued', 'running'].includes(runData.status);

  useEffect(() => {
    form.setFieldsValue({
      sample_count: DEFAULT_OPTIONS.sample_count,
      runtime_collect_seconds: DEFAULT_RUNTIME_SECONDS,
    });
  }, [form]);

  useEffect(() => {
    if (!releaseId) return;
    fetchReleaseDetail(releaseId)
      .then(setReleaseInfo)
      .catch(() => setReleaseInfo(null));
  }, [releaseId]);

  useEffect(() => {
    if (!releaseId) return;
    let cancelled = false;
    (async () => {
      try {
        const latest = await fetchLatestSandboxRun(releaseId);
        if (!latest?.task_id) return;
        const detail = await fetchSandboxRun(latest.task_id);
        if (!cancelled) {
          setRunData(detail);
        }
      } catch (error) {
        // ignore 404
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [releaseId]);

  const refreshHistory = useCallback(
    async (pageNumber = 1) => {
      if (!releaseId) return;
      const safePage = Math.max(1, pageNumber);
      setHistoryLoading(true);
      try {
        const fetchLimit = Math.min(
          safePage * HISTORY_PAGE_SIZE,
          HISTORY_FETCH_LIMIT,
        );
        const resp = await fetchSandboxHistory(releaseId, fetchLimit);
        setHistoryData({
          total: resp?.total ?? resp?.items?.length ?? 0,
          items: resp?.items ?? [],
        });
      } catch (error) {
        message.error(error?.error?.message || t('sandbox.historyFetchFailed'));
      } finally {
        setHistoryLoading(false);
      }
    },
    [releaseId, t],
  );

  useEffect(() => {
    refreshHistory(historyPage);
  }, [historyPage, refreshHistory]);

  useEffect(() => {
    setHistoryPage(1);
  }, [releaseId]);

  useEffect(() => {
    if (!runData?.task_id) {
      setPolling(false);
      return;
    }
    if (!pollingStatuses.has(runData.status)) {
      setPolling(false);
      return;
    }
    setPolling(true);
    const timer = setInterval(async () => {
      try {
        const latest = await fetchSandboxRun(runData.task_id);
        setRunData(latest);
        if (!pollingStatuses.has(latest?.status)) {
          setPolling(false);
          setHistoryPage(1);
          refreshHistory(1);
        }
      } catch (error) {
        setPolling(false);
      }
    }, 1500);
    return () => clearInterval(timer);
  }, [runData?.task_id, runData?.status, refreshHistory]);

  useEffect(() => {
    if (!runData?.status) {
      setQueuePosition(null);
      setAutoFollowStage(false);
      return;
    }
    const isActive = pollingStatuses.has(runData.status);
    if (!isActive) {
      setQueuePosition(null);
    }
    setAutoFollowStage(isActive);
  }, [runData?.status]);

  useEffect(() => {
    if (!runData?.task_id || filteredStages.length === 0) {
      setSelectedStage((prev) => (prev === null ? prev : null));
      setStageLog((prev) => (prev === '' ? prev : ''));
      setStageLogMeta((prev) => {
        if (
          prev.stage === EMPTY_STAGE_LOG_META.stage &&
          prev.logPath === EMPTY_STAGE_LOG_META.logPath &&
          prev.fetchedAt === EMPTY_STAGE_LOG_META.fetchedAt
        ) {
          return prev;
        }
        return { ...EMPTY_STAGE_LOG_META };
      });
      lastTaskIdRef.current = null;
      return;
    }
    if (lastTaskIdRef.current === runData.task_id) {
      return;
    }
    lastTaskIdRef.current = runData.task_id;
    const initialStage = filteredStages[0].stage;
    setSelectedStage(initialStage);
    setStageLog('');
    setStageLogMeta({ stage: initialStage, logPath: '', fetchedAt: null });
    autoScrollTaskRef.current = runData.task_id;
  }, [filteredStages, runData?.task_id]);

  useEffect(() => {
    if (!runData?.task_id) return;
    const failureStage =
      filteredStages.find((stage) => stage.status === 'failed' || stage.status === 'stopped') ||
      null;
    if (failureStage && runData.status === 'failed') {
      setSelectedStage(failureStage.stage);
      const scrollKey = `${runData.task_id}-${failureStage.stage}`;
      if (autoScrollTaskRef.current !== scrollKey) {
        autoScrollTaskRef.current = scrollKey;
        requestAnimationFrame(() => {
          const target = document.getElementById(`sandbox-stage-${failureStage.stage}`);
          if (target) {
            target.scrollIntoView({ behavior: 'smooth', block: 'center' });
          }
        });
      }
    }
  }, [filteredStages, runData?.status, runData?.task_id]);

  useEffect(() => {
    if (!autoFollowStage) return;
    if (!filteredStages.length) return;
    const runningStage =
      filteredStages.find((stage) => stage.status === 'running') ||
      filteredStages.find((stage) => stage.status === 'pending');
    if (runningStage && runningStage.stage !== selectedStage) {
      setSelectedStage(runningStage.stage);
      setStageLog('');
      setStageLogMeta({ stage: runningStage.stage, logPath: '', fetchedAt: null });
    }
  }, [autoFollowStage, filteredStages, selectedStage]);

  useEffect(() => {
    if (!selectedStage || !runData?.task_id) {
      return;
    }
    fetchStageLog(selectedStage);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedStage, runData?.task_id]);

  const fetchStageLog = async (stageKey, options = {}) => {
    if (!stageKey || !runData?.task_id) return;
    const { silent = false } = options;
    if (!silent) {
      setStageLogLoading(true);
    }
    try {
      const resp = await fetchSandboxStageLogs(runData.task_id, stageKey);
      setStageLog(resp?.content || '');
      setStageLogMeta({
        stage: stageKey,
        logPath: resp?.log_path || '',
        fetchedAt: Date.now(),
      });
    } catch (error) {
      if (!silent) {
        message.error(t('sandbox.logFetchFailed'));
      }
    } finally {
      if (!silent) {
        setStageLogLoading(false);
      }
    }
  };

  useEffect(() => {
    const stageInfo = stages.find((item) => item.stage === selectedStage);
    const shouldAutoRefresh =
      stageInfo &&
      stageInfo.status === 'running' &&
      runData?.task_id &&
      pollingStatuses.has(runData?.status);

    if (!shouldAutoRefresh) {
      if (logAutoRefreshRef.current) {
        clearInterval(logAutoRefreshRef.current);
        logAutoRefreshRef.current = null;
      }
      return;
    }

    const tick = () => {
      fetchStageLog(stageInfo.stage, { silent: true });
    };
    tick();
    logAutoRefreshRef.current = setInterval(tick, 500);
    return () => {
      if (logAutoRefreshRef.current) {
        clearInterval(logAutoRefreshRef.current);
        logAutoRefreshRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedStage, stages, runData?.status, runData?.task_id]);

  useEffect(() => {
    return () => {
      if (logAutoRefreshRef.current) {
        clearInterval(logAutoRefreshRef.current);
        logAutoRefreshRef.current = null;
      }
    };
  }, []);

  const handleStartSandbox = async () => {
    if (!releaseId) return;
    try {
      const values = await form.validateFields();
      setLoading(true);
      const runtimeSeconds =
        typeof values.runtime_collect_seconds === 'number' && !Number.isNaN(values.runtime_collect_seconds)
          ? values.runtime_collect_seconds
          : DEFAULT_RUNTIME_SECONDS;
      const runtimeCollectMs = Math.min(Math.max(Math.round(runtimeSeconds * 1000), 1000), 60000);
      const payload = {
        release_id: releaseId,
        overrides: [],
        options: {
          sample_count: values.sample_count,
          startup_timeout_ms: DEFAULT_OPTIONS.startup_timeout_ms,
          runtime_collect_ms: runtimeCollectMs,
          keep_workspace: DEFAULT_OPTIONS.keep_workspace,
          wpgen_timeout_ms: DEFAULT_OPTIONS.wpgen_timeout_ms,
        },
      };
      const resp = await createSandboxRun(payload);
      setQueuePosition(resp?.queue_position ?? null);
      if (resp?.queue_position > 0) {
        message.info(t('sandbox.queueInfo', { position: resp.queue_position }));
      }
      if (resp?.task_id) {
        const detail = await fetchSandboxRun(resp.task_id);
        setRunData(detail);
      } else {
        setRunData(resp);
      }
      setHistoryPage(1);
      refreshHistory(1);
    } catch (error) {
      if (error?.error?.code === 'SANDBOX_QUEUE_FULL') {
        message.warning(error?.error?.message || t('sandbox.queueFull'));
      } else {
        message.error(error?.error?.message || t('sandbox.startFailed'));
      }
    } finally {
      setLoading(false);
    }
  };

  const handleStopSandbox = async () => {
    if (!runData?.task_id) return;
    setLoading(true);
    try {
      await stopSandboxRun(runData.task_id);
      const latest = await fetchSandboxRun(runData.task_id);
      setRunData(latest);
      setHistoryPage(1);
      refreshHistory(1);
    } catch (error) {
      const code = error?.error?.code;
      if (code === 'TASK_NOT_RUNNING') {
        message.warning(error?.error?.message || t('sandbox.taskNotRunning'));
      } else if (code === 'NOT_FOUND') {
        message.error(t('sandbox.taskNotFound'));
      } else {
        message.error(error?.error?.message || t('sandbox.stopFailed'));
      }
    } finally {
      setLoading(false);
    }
  };

  const handleSelectHistoryRun = async (taskId) => {
    if (!taskId || taskId === runData?.task_id) return;
    try {
      const detail = await fetchSandboxRun(taskId);
      setRunData(detail);
    } catch (error) {
      message.error(error?.error?.message || t('sandbox.historyLoadFailed'));
    }
  };

  const handleHistoryPageChange = (pageNumber) => {
    const safePage = Math.min(pageNumber, MAX_HISTORY_PAGE);
    setHistoryPage(safePage);
  };

  const handleTimelineSelect = (stageKey) => {
    if (!stageKey) return;
    if (stageKey === selectedStage) {
      fetchStageLog(stageKey);
      return;
    }
    setSelectedStage(stageKey);
    setStageLog('');
    setStageLogMeta({ stage: stageKey, logPath: '', fetchedAt: null });
    requestAnimationFrame(() => {
      if (stageLogCardRef.current) {
        stageLogCardRef.current.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    });
  };

  const handleRefreshLog = () => {
    if (selectedStage) {
      fetchStageLog(selectedStage);
    }
  };

  return (
    <div className="prepublish-page" style={{ padding: 24 }}>
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space size="middle" align="center" wrap>
          <Button icon={<ArrowLeftOutlined />} onClick={() => navigate(-1)}>
            {t('systemRelease.backToList')}
          </Button>
          <Title level={4} style={{ margin: 0 }}>
            {t('sandbox.title')} / {releaseInfo?.version || `#${releaseId}`}
          </Title>
          {statusTag}
          <Space>
            <Button
              type="primary"
              icon={<PlayCircleOutlined />}
              onClick={handleStartSandbox}
              loading={loading && !polling}
            >
              {runData ? t('sandbox.rerun') : t('sandbox.startSandbox')}
            </Button>
            <Button
              icon={<PauseCircleOutlined />}
              onClick={handleStopSandbox}
              disabled={!canStop}
              loading={loading && pollingStatuses.has(runData?.status)}
            >
              {t('sandbox.stopExecution')}
            </Button>
          </Space>
        </Space>

        {polling && (
          <Alert
            message={t('sandbox.runningNoticeTitle')}
            description={t('sandbox.runningAlert')}
            type="info"
            showIcon
          />
        )}

        {queueAlertNeeded && (
          <Alert
            message={t('sandbox.queueWaitingTitle')}
            description={t('sandbox.queueInfo', { position: queuePosition ?? 1 })}
            type="warning"
            showIcon
          />
        )}

        <Row gutter={[16, 16]}>
          <Col xs={24} lg={8} style={{ display: 'flex' }}>
            <SandboxHistoryList
              history={historyItems}
              loading={historyLoading}
              activeTaskId={runData?.task_id}
              onSelect={handleSelectHistoryRun}
              t={t}
              totalCount={historyData.total}
              cardStyle={{ width: '100%', height: '100%' }}
              page={historyPage}
              pageSize={HISTORY_PAGE_SIZE}
              maxTotal={HISTORY_FETCH_LIMIT}
              onPageChange={handleHistoryPageChange}
            />
          </Col>
          <Col xs={24} lg={8} style={{ display: 'flex' }}>
            <Card title={t('sandbox.runPreparation')} style={{ width: '100%' }}>
              <Space direction="vertical" style={{ width: '100%' }} size="middle">
                <div>
                  <Text type="secondary">{t('sandbox.currentVersion')}</Text>
                  <Paragraph style={{ marginBottom: 0 }}>{currentVersion}</Paragraph>
                </div>
                <div>
                  <Text type="secondary">{t('sandbox.baselineLabel')}</Text>
                  <Paragraph style={{ marginBottom: 0 }}>{baselineVersion}</Paragraph>
                </div>
                <Paragraph type="secondary">{t('sandbox.baselineDescription')}</Paragraph>
                <Form layout="vertical" form={form}>
                  <Form.Item
                    name="sample_count"
                    label={t('sandbox.sampleCount')}
                    rules={[{ required: true, message: t('sandbox.sampleCount') }]}
                  >
                    <InputNumber min={1} max={10000} style={{ width: '100%' }} />
                  </Form.Item>
                  <Form.Item
                    name="runtime_collect_seconds"
                    label={t('sandbox.runtimeCollectLabel')}
                    rules={[
                      { required: true, message: t('sandbox.runtimeCollectLabel') },
                      {
                        type: 'number',
                        min: 1,
                        max: 60,
                        message: t('sandbox.runtimeCollectRangeMessage'),
                      },
                    ]}
                  >
                    <InputNumber min={1} max={60} style={{ width: '100%' }} />
                  </Form.Item>
                </Form>
              </Space>
            </Card>
          </Col>
          <Col xs={24} lg={8} style={{ display: 'flex' }}>
            <SandboxResultPanel
              runData={runData}
              conclusion={conclusion}
              t={t}
              formatStageDuration={formatStageDuration}
              formatDisplayTime={formatDisplayTime}
              failureSummary={failureSummary}
              cardStyle={{ width: '100%', height: '100%' }}
            />
          </Col>
        </Row>

        <Row gutter={[16, 16]}>
          <Col xs={24} lg={12} style={{ display: 'flex' }}>
            <Card title={t('sandbox.executionStages')} style={{ width: '100%' }}>
              <SandboxStageTimeline
                stages={stages}
                selectedStage={selectedStage}
                onSelectStage={handleTimelineSelect}
                t={t}
                formatStageDuration={formatStageDuration}
                visibleStages={VISIBLE_STAGE_KEYS}
              />
            </Card>
          </Col>
          <Col xs={24} lg={12} style={{ display: 'flex' }}>
            <div ref={stageLogCardRef} style={{ width: '100%' }}>
              <SandboxLogViewer
                selectedStageInfo={selectedStageInfo}
                stageLog={stageLog}
                stageLogLoading={stageLogLoading}
                stageLogMeta={stageLogMeta}
                onRefresh={handleRefreshLog}
                t={t}
                formatStageDuration={formatStageDuration}
                formatDisplayTime={formatDisplayTime}
                cardStyle={{ width: '100%', height: '100%' }}
                cardId="sandbox-log-viewer"
              />
            </div>
          </Col>
        </Row>
      </Space>
    </div>
  );
}

export default PrepublishPage;
