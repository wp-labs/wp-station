import React, { startTransition, useDeferredValue, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { UpOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { useNavigate, useParams } from 'react-router-dom';
import { FloatButton } from 'antd';
import { useSystem } from '@/contexts/SystemContext';
import { fetchReleaseDetail, fetchReleaseDiff } from '@/services/release';
import DiffViewer from '@/components/diff/DiffViewer';
import { parseDiffText } from '@/components/diff/diffUtils';

const DIFF_BATCH_SIZE = 10;
const RELEASE_DETAIL_COLLAPSED_LINE_THRESHOLD = 100;

function sanitizeAnchorSegment(value = '') {
  return String(value)
    .trim()
    .replace(/[^a-zA-Z0-9/_-]+/g, '-')
    .replace(/[\\/]+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .toLowerCase();
}

function buildDiffAnchorId(releaseGroup, filePath, index) {
  return `release-diff-${sanitizeAnchorSegment(releaseGroup)}-${index}-${sanitizeAnchorSegment(filePath)}`;
}

function adaptDiffFiles(files = [], startIndex = 0) {
  return files.map((file, index) => {
    const parsedFiles = file.diff_text ? parseDiffText(file.diff_text) : [];
    return {
      release_group: file.release_group || 'draft',
      file_path: file.file_path,
      old_path: file.old_path,
      change_type: file.change_type || 'modify',
      diff_text: file.diff_text,
      parsedDiff: parsedFiles?.[0] || null,
      diff_anchor_id: buildDiffAnchorId(
        file.release_group || 'draft',
        file.file_path,
        startIndex + index,
      ),
    };
  });
}

function normalizeChangeBucket(changeType) {
  if (changeType === 'add') {
    return 'added';
  }
  if (changeType === 'delete') {
    return 'deleted';
  }
  return 'modified';
}

function buildDiffOverviewBucketsFromStats(files = []) {
  return files.reduce((buckets, file) => {
    const key = normalizeChangeBucket(file.change_type);
    buckets[key] += 1;
    return buckets;
  }, { modified: 0, deleted: 0, added: 0 });
}

function getFileChangeCode(changeType) {
  if (changeType === 'add') {
    return 'A';
  }
  if (changeType === 'delete') {
    return 'D';
  }
  return 'M';
}

function getFileChangeCodeStyle(changeType) {
  if (changeType === 'add') {
    return {
      color: '#17b26a',
      background: 'rgba(23, 178, 106, 0.12)',
      borderColor: 'rgba(23, 178, 106, 0.24)',
    };
  }
  if (changeType === 'delete') {
    return {
      color: '#f1554c',
      background: 'rgba(241, 85, 76, 0.12)',
      borderColor: 'rgba(241, 85, 76, 0.24)',
    };
  }
  return {
    color: '#f79009',
    background: 'rgba(247, 144, 9, 0.12)',
    borderColor: 'rgba(247, 144, 9, 0.24)',
  };
}

function ReleaseDetailPage() {
  const { t } = useTranslation();
  const { id: releaseId } = useParams();
  const navigate = useNavigate();
  const { currentSystem, decoratePath } = useSystem();
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState(null);
  const [error, setError] = useState(null);
  const [focusedDiffAnchorId, setFocusedDiffAnchorId] = useState('');
  const [diffState, setDiffState] = useState({
    groups: [],
    files: [],
    stats: null,
    totalFiles: 0,
    hasMore: false,
    loading: false,
    loadingMore: false,
    initialized: false,
    error: null,
  });
  const loadMoreSentinelRef = useRef(null);
  const diffRequestPendingRef = useRef(false);
  const appendScrollRestoreRef = useRef(null);
  const deferredDiffFiles = useDeferredValue(diffState.files);

  const loadDetail = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await fetchReleaseDetail(releaseId, currentSystem);
      setDetail(response);
    } catch (err) {
      setError({
        message: err.message || t('systemRelease.detailLoadFailed'),
      });
    } finally {
      setLoading(false);
    }
  };

  const loadDiffPage = async ({ append, offset }) => {
    if (diffRequestPendingRef.current) {
      return;
    }

    if (append && typeof window !== 'undefined') {
      appendScrollRestoreRef.current = {
        left: window.scrollX,
        top: window.scrollY,
      };
    }

    diffRequestPendingRef.current = true;
    setDiffState((prev) => ({
      ...prev,
      loading: append ? prev.loading : true,
      loadingMore: append,
      error: append ? prev.error : null,
    }));

    try {
      const response = await fetchReleaseDiff(
        releaseId,
        {
          offset,
          limit: DIFF_BATCH_SIZE,
        },
        currentSystem,
      );
      const nextFiles = adaptDiffFiles(response?.files || [], offset);

      startTransition(() => {
        setDiffState((prev) => ({
          groups: Array.isArray(response?.groups) ? response.groups : prev.groups,
          files: append ? [...prev.files, ...nextFiles] : nextFiles,
          stats: response?.stats || prev.stats,
          totalFiles: response?.total_files ?? prev.totalFiles,
          hasMore: Boolean(response?.has_more),
          loading: false,
          loadingMore: false,
          initialized: true,
          error: null,
        }));
      });
    } catch (diffError) {
      setDiffState((prev) => ({
        ...prev,
        loading: false,
        loadingMore: false,
        initialized: true,
        error: {
          message: diffError.message || t('systemRelease.diffLoadFailed'),
        },
      }));
    } finally {
      diffRequestPendingRef.current = false;
    }
  };

  useLayoutEffect(() => {
    if (diffState.loadingMore || !appendScrollRestoreRef.current || typeof window === 'undefined') {
      return;
    }

    const { left, top } = appendScrollRestoreRef.current;
    appendScrollRestoreRef.current = null;

    const restoreScroll = () => {
      window.scrollTo(left, top);
    };

    restoreScroll();
    const frameId = window.requestAnimationFrame(restoreScroll);
    return () => window.cancelAnimationFrame(frameId);
  }, [diffState.files.length, diffState.loadingMore]);

  useEffect(() => {
    loadDetail();
  }, [currentSystem, releaseId]);

  useEffect(() => {
    setDiffState({
      groups: [],
      files: [],
      stats: null,
      totalFiles: 0,
      hasMore: false,
      loading: false,
      loadingMore: false,
      initialized: false,
      error: null,
    });
    loadDiffPage({ append: false, offset: 0 });
  }, [currentSystem, releaseId]);

  const getReleaseGroupTitle = (releaseGroup) => {
    if (releaseGroup === 'models') {
      return t('systemRelease.groupModels');
    }
    if (releaseGroup === 'infra') {
      return t('systemRelease.groupInfra');
    }
    if (releaseGroup === 'all') {
      return t('systemRelease.groupAll');
    }
    return t('systemRelease.draftLabel');
  };

  const diffGroups = useMemo(() => {
    const loadedByGroup = diffState.files.reduce((accumulator, file) => {
      const key = file.release_group || 'draft';
      if (!accumulator[key]) {
        accumulator[key] = [];
      }
      accumulator[key].push(file);
      return accumulator;
    }, {});

    return Array.isArray(diffState.groups)
      ? diffState.groups.map((group) => ({
          ...group,
          loadedFiles: loadedByGroup[group.release_group] || [],
          loadedCount: (loadedByGroup[group.release_group] || []).length,
          changeBuckets: buildDiffOverviewBucketsFromStats(loadedByGroup[group.release_group] || []),
        }))
      : [];
  }, [diffState.files, diffState.groups]);

  useEffect(() => {
    if (!loadMoreSentinelRef.current || !diffState.hasMore || diffState.loading || diffState.loadingMore) {
      return undefined;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const [entry] = entries;
        if (!entry?.isIntersecting || diffRequestPendingRef.current) {
          return;
        }
        loadDiffPage({ append: true, offset: diffState.files.length });
      },
      {
        rootMargin: '240px 0px',
      },
    );

    observer.observe(loadMoreSentinelRef.current);
    return () => observer.disconnect();
  }, [diffState.files.length, diffState.hasMore, diffState.loading, diffState.loadingMore]);

  const getReleaseStatusMeta = () => {
    const normalizedStatus = String(detail?.status || '').toUpperCase();
    const releaseGroup = detail?.release_group || 'draft';

    if (normalizedStatus === 'RUNNING') {
      return { className: 'is-running', text: t('systemRelease.statusRunning') };
    }
    if (normalizedStatus === 'FAIL' || normalizedStatus === 'PARTIAL_FAIL') {
      return { className: 'is-fail', text: t('systemRelease.statusFailed') };
    }
    if (normalizedStatus === 'PASS') {
      if (releaseGroup === 'models') {
        return { className: 'is-pass', text: t('systemRelease.statusPublishedModels') };
      }
      if (releaseGroup === 'infra') {
        return { className: 'is-pass', text: t('systemRelease.statusPublishedInfra') };
      }
      if (releaseGroup === 'all') {
        return { className: 'is-pass', text: t('systemRelease.statusPublishedAll') };
      }
    }
    return { className: 'is-wait', text: t('systemRelease.statusDraft') };
  };

  const translateStageLabel = (label) => {
    const stageMap = {
      沙盒: t('systemRelease.stageSandbox'),
      发布: t('systemRelease.stagePublish'),
      发布规则: t('systemRelease.stagePublishModels'),
      发布设施: t('systemRelease.stagePublishInfra'),
      发布全量: t('systemRelease.stagePublishAll'),
      拉取历史副本: t('systemRelease.stageRestoreFetchHistory'),
      '发布还原 Git': t('systemRelease.stageRestoreGit'),
      发布还原规则配置: t('systemRelease.stageRestoreModels'),
      发布还原设施配置: t('systemRelease.stageRestoreInfra'),
      准备: t('systemRelease.stagePrepare'),
      调用客户端: t('systemRelease.stageCallClient'),
      运行状态: t('systemRelease.stageRuntime'),
    };
    return stageMap[label] || label;
  };

  /** 将设备和还原任务返回的英文状态转换为页面展示文案。 */
  const getDisplayStatus = (status) => {
    const normalizedStatus = String(status || '').toUpperCase();
    const statusMap = {
      SUCCESS: t('systemRelease.statusSuccess'),
      PASS: t('systemRelease.statusPass'),
      FAIL: t('systemRelease.statusFail'),
      PARTIAL_FAIL: t('systemRelease.statusPartialFail'),
      RUNNING: t('systemRelease.statusRunning'),
      QUEUED: t('systemRelease.statusQueued'),
      WAIT: t('systemRelease.statusWait'),
      INIT: t('systemRelease.statusInit'),
      ROLLED_BACK: t('systemRelease.statusRolledBack'),
      COMPLETED: t('systemRelease.statusCompleted'),
    };
    return statusMap[normalizedStatus] || status || '—';
  };

  /** 将还原任务阶段转换为用户可读的中文或英文文案。 */
  const getRestorePhaseLabel = (phase) => {
    const normalizedPhase = String(phase || '').toUpperCase();
    const phaseMap = {
      QUEUED: 'restorePhaseQueued',
      PREPARING: 'restorePhasePreparing',
      TAG_READY: 'restorePhaseTagReady',
      MODELS_RUNNING: 'restorePhaseModelsRunning',
      MODELS_SUCCESS: 'restorePhaseModelsSuccess',
      INFRA_RUNNING: 'restorePhaseInfraRunning',
      INFRA_SUCCESS: 'restorePhaseInfraSuccess',
      PROMOTING: 'restorePhasePromoting',
      PROMOTE_PENDING: 'restorePhasePromotePending',
      ROLLBACK_RUNNING: 'restorePhaseRollbackRunning',
      ROLLBACK_FAILED: 'restorePhaseRollbackFailed',
      PREPARE_FAILED: 'restorePhasePrepareFailed',
      COMPLETED: 'restorePhaseCompleted',
    };
    return phaseMap[normalizedPhase]
      ? t(`systemRelease.${phaseMap[normalizedPhase]}`)
      : phase || '—';
  };

  const renderStageChip = (stage, index) => {
    const status = String(stage?.status || '').toLowerCase();
    let className = 'stage-icon';
    let icon = '>>';
    if (status === 'pass') {
      className = 'stage-icon is-pass';
      icon = '✓';
    } else if (status === 'fail') {
      className = 'stage-icon is-fail';
      icon = '✗';
    } else if (status === 'running') {
      className = 'stage-icon is-running';
      icon = '…';
    }
    return (
      <span key={index} className="stage-chip">
        <span className={className} title={translateStageLabel(stage.label || '')}>
          {icon}
        </span>
        <span className="stage-name">{translateStageLabel(stage.label || '')}</span>
      </span>
    );
  };

  const handleJumpToDiff = (anchorId) => {
    if (!anchorId) {
      return;
    }

    setFocusedDiffAnchorId(anchorId);

    const target = document.getElementById(anchorId);
    if (!target) {
      return;
    }

    const nextUrl = `${window.location.pathname}${window.location.search}#${anchorId}`;
    window.history.replaceState(null, '', nextUrl);
    target.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  // 必须在条件渲染之前固定调用，避免详情数据加载前后改变 Hook 顺序。
  const machineGroups = useMemo(() => {
    const grouped = new Map();
    (detail?.devices || []).forEach((device) => {
      const key = String(device.device_id ?? `${device.ip}:${device.port}`);
      if (!grouped.has(key)) {
        grouped.set(key, {
          key,
          deviceName: device.device_name,
          ip: device.ip,
          port: device.port,
          targets: {},
        });
      }
      const machine = grouped.get(key);
      const group = device.release_group || 'draft';
      machine.targets[group] = device;
    });
    return Array.from(grouped.values());
  }, [detail?.devices]);

  if (loading) {
    return (
      <div className="panel is-visible">
        <div style={{ padding: '40px', textAlign: 'center' }}>{t('common.loading')}</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="panel is-visible">
        <div style={{ padding: '40px', textAlign: 'center' }}>
          <div>{error.message}</div>
          <button
            type="button"
            className="btn ghost"
            onClick={() => navigate(decoratePath('/system-release'))}
          >
            {t('systemRelease.backToList')}
          </button>
        </div>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="panel is-visible">
        <div style={{ padding: '40px', textAlign: 'center' }}>{t('common.noData')}</div>
      </div>
    );
  }

  const statusMeta = getReleaseStatusMeta();
  const isPublished = ['PASS', 'FAIL', 'PARTIAL_FAIL'].includes(String(detail.status || '').toUpperCase());
  const releaseGroupLabel =
    detail.release_group === 'models'
      ? t('systemRelease.groupModels')
      : detail.release_group === 'infra'
        ? t('systemRelease.groupInfra')
        : detail.release_group === 'all'
          ? t('systemRelease.groupAll')
          : t('systemRelease.draftLabel');
  const totalDiffStats = diffState.stats || { files_changed: 0, insertions: 0, deletions: 0 };
  const loadedDiffFileCount = diffState.files.length;
  const totalDiffFileCount = diffState.totalFiles || 0;
  const hasDiffFiles = totalDiffFileCount > 0;

  const getDeviceStatusClass = (deviceStatus) => {
    if (deviceStatus === 'SUCCESS' || deviceStatus === 'ROLLED_BACK') {
      return 'is-pass';
    }
    if (deviceStatus === 'FAIL') {
      return 'is-fail';
    }
    if (['RUNNING', 'QUEUED', 'ROLLBACKING'].includes(deviceStatus)) {
      return 'is-running';
    }
    return 'is-wait';
  };

  const getMachineStatus = (targets, releaseGroup, restoreStatus) => {
    const requiredGroups = releaseGroup === 'all'
      ? ['models', 'infra']
      : releaseGroup && releaseGroup !== 'draft'
        ? [releaseGroup]
        : [];
    const statuses = Object.values(targets)
      .filter(Boolean)
      .map((target) => String(target.status || '').toUpperCase());
    const failedRestore = ['FAIL', 'PARTIAL_FAIL', 'ROLLBACK_FAILED'].includes(
      String(restoreStatus || '').toUpperCase(),
    );
    if (failedRestore || requiredGroups.some((group) => !targets[group])) {
      return 'FAIL';
    }
    if (statuses.some((status) => status === 'FAIL')) {
      return 'FAIL';
    }
    if (statuses.some((status) => ['RUNNING', 'QUEUED', 'ROLLBACKING'].includes(status))) {
      return 'RUNNING';
    }
    if (statuses.length > 0 && statuses.every((status) => ['SUCCESS', 'ROLLED_BACK'].includes(status))) {
      return 'SUCCESS';
    }
    return '—';
  };

  const renderDeviceTargetPanel = (target, group) => {
    if (!target) {
      const releaseGroup = detail.restore_info?.release_group || detail.release_group;
      const groupExpected = releaseGroup === 'all' || releaseGroup === group;
      const restoreFailed = detail.restore_info
        && ['FAIL', 'PARTIAL_FAIL', 'ROLLBACK_FAILED'].includes(
          String(detail.restore_info.status || '').toUpperCase(),
        );
      return (
        <section
          key={group}
          style={{
            minHeight: '150px',
            padding: '14px 16px',
            border: '1px solid #eef0f4',
            borderRadius: '10px',
            background: '#fafbfc',
          }}
        >
          <h5 style={{ margin: '0 0 12px', fontSize: '14px', color: '#344054' }}>
            {t('systemRelease.deviceReleaseGroup', {
              group: getReleaseGroupTitle(group),
            })}
          </h5>
          <div style={{ fontSize: '13px', color: '#98a2b3' }}>
            {groupExpected && restoreFailed
              ? t('systemRelease.restoreGroupNotStarted')
              : t('systemRelease.notIncludedInRelease')}
          </div>
        </section>
      );
    }

    const deviceStatus = String(target.status || '').toUpperCase();
    const deviceClass = getDeviceStatusClass(deviceStatus);

    return (
      <section
        key={group}
        style={{
          minWidth: 0,
          padding: '14px 16px',
          border: '1px solid #eef0f4',
          borderRadius: '10px',
          background: '#fafbfc',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '10px' }}>
          <h5 style={{ margin: 0, fontSize: '14px', color: '#344054' }}>
            {t('systemRelease.deviceReleaseGroup', {
              group: getReleaseGroupTitle(group),
            })}
          </h5>
          <span className={`release-status ${deviceClass}`}>{getDisplayStatus(deviceStatus)}</span>
        </div>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
            gap: '6px 12px',
            marginBottom: '10px',
            fontSize: '12px',
            color: '#667085',
          }}
        >
          <span>{t('systemRelease.currentConfigVersion')}: {target.config_version || '—'}</span>
          <span>{t('systemRelease.targetConfigVersion')}: {target.target_config_version || '—'}</span>
        </div>
        {Array.isArray(target.stage_trace) && target.stage_trace.length > 0 ? (
          <div className="summary-stages">{target.stage_trace.map(renderStageChip)}</div>
        ) : null}
        {(target.operation || target.attempt_no) && (
          <div style={{ marginTop: '8px', fontSize: '12px', color: '#667085' }}>
            {t('systemRelease.releaseOperation')}: {target.operation || 'publish'} ·{' '}
            {t('systemRelease.releaseAttempt', { count: target.attempt_no || 1 })}
          </div>
        )}
        {target.request_summary ? (
          <div style={{ marginTop: '6px', fontSize: '12px', color: '#667085', whiteSpace: 'pre-wrap' }}>
            {t('systemRelease.releaseRequest')}: {target.request_summary}
          </div>
        ) : null}
        {target.response_status || target.response_summary ? (
          <div style={{ marginTop: '6px', fontSize: '12px', color: '#667085', whiteSpace: 'pre-wrap' }}>
            {t('systemRelease.releaseResponse')}: {target.response_status || '—'}{' '}
            {target.response_summary || ''}
          </div>
        ) : null}
        {target.error_message ? (
          <div style={{ marginTop: '8px', fontSize: '13px', color: '#f1554c' }}>
            {target.error_message}
          </div>
        ) : null}
      </section>
    );
  };

  return (
    <div className="panel is-visible">
      <div className="release-detail">
        <header className="release-detail-header">
          <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
          <button
            type="button"
            className="btn ghost"
            onClick={() => navigate(decoratePath('/system-release'))}
          >
            {t('systemRelease.backToList')}
          </button>
            <button
              type="button"
              className="btn"
              onClick={() => navigate(decoratePath(`/system-release/${releaseId}/prepublish`))}
            >
              {t(isPublished ? 'sandbox.prepublishDetail' : 'sandbox.startSandbox')}
            </button>
          </div>
          <h3 id="detail-title">
            {detail.restore_info
              ? `${t('systemRelease.restoreTaskTitle')} · ${t('systemRelease.versionDetail', { version: detail.version })}`
              : t('systemRelease.versionDetail', { version: detail.version })}
          </h3>
        </header>

        <div className="release-summary">
          <div className="summary-item">
            <span className="summary-label">{t('systemRelease.status')}</span>
            <span className={`summary-value ${statusMeta.className}`}>{statusMeta.text}</span>
          </div>
          <div className="summary-item">
            <span className="summary-label">
              {detail.restore_info
                ? t('systemRelease.restoreCurrentVersion')
                : t('systemRelease.version')}
            </span>
            <span className="summary-value">{detail.version || '—'}</span>
          </div>
          <div className="summary-item">
            <span className="summary-label">{t('systemRelease.system')}</span>
            <span className="summary-value">
              {t(`navigation.system.${detail.system || currentSystem}`)}
            </span>
          </div>
          <div className="summary-item">
            <span className="summary-label">{t('systemRelease.releaseGroup')}</span>
            <span className="summary-value">{releaseGroupLabel}</span>
          </div>
          <div className="summary-item">
            <span className="summary-label">{t('systemRelease.remark')}</span>
            <span className="summary-value">{detail.pipeline || '—'}</span>
          </div>
          <div className="summary-item">
            <span className="summary-label">{t('systemRelease.stages')}</span>
            <div className="summary-stages">
              {(detail.stages || []).map(renderStageChip)}
            </div>
          </div>
        </div>

        {detail.restore_info ? (
          <div
            className="release-devices"
            style={{
              display: 'grid',
              gridTemplateColumns: 'minmax(0, 1fr) auto',
              alignItems: 'center',
              gap: '10px 16px',
              margin: '20px 0',
              padding: '16px 18px',
              border: '1px solid #b8d1ff',
              borderRadius: '10px',
              background: '#eef5ff',
            }}
          >
            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', minWidth: 0 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '10px', whiteSpace: 'nowrap' }}>
                <strong style={{ color: '#1d4ed8', fontSize: '15px' }}>
                  {t('systemRelease.restoreTaskTitle')}
                </strong>
                <span
                  className={`release-status ${['FAIL', 'PARTIAL_FAIL', 'ROLLBACK_FAILED'].includes(String(detail.restore_info.status || '').toUpperCase())
                    ? 'is-fail'
                    : String(detail.restore_info.status || '').toUpperCase() === 'PASS'
                      ? 'is-pass'
                      : 'is-running'}`}
                >
                  {getDisplayStatus(detail.restore_info.status)}
                </span>
              </div>
              <div style={{ display: 'flex', gap: '16px', flexWrap: 'wrap', fontSize: '13px', color: '#475467' }}>
                <span>{t('systemRelease.restoreSourceRelease')}: {detail.restore_info.source_version}</span>
                <span>{t('systemRelease.restoreCurrentVersion')}: {detail.version}</span>
                <span>{t('systemRelease.restoreTaskScope')}: {releaseGroupLabel}</span>
                <span>{t('systemRelease.restoreTaskPhase')}: {getRestorePhaseLabel(detail.restore_info.phase)}</span>
              </div>
            </div>
            <button
              type="button"
              className="btn ghost"
              style={{ gridColumn: '2', justifySelf: 'end', fontSize: '12px', padding: '4px 12px' }}
              onClick={() => navigate(decoratePath(`/system-release/${detail.restore_info.source_release_id}`))}
            >
              {t('systemRelease.viewSourceRelease')}
            </button>
          </div>
        ) : null}

        {Array.isArray(detail.restore_attempts) && detail.restore_attempts.length > 0 ? (
          <div className="release-devices" style={{ margin: '20px 0' }}>
            <header className="release-diff-header">
              <h4>{t('systemRelease.restoreHistory')}</h4>
            </header>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginTop: 12 }}>
              {detail.restore_attempts.map((attempt) => {
                const status = String(attempt.status || '').toUpperCase();
                const statusClass = status === 'PASS'
                  ? 'is-pass'
                  : ['FAIL', 'PARTIAL_FAIL', 'ROLLBACK_FAILED'].includes(status)
                    ? 'is-fail'
                    : 'is-running';
                return (
                  <button
                    key={attempt.job_id}
                    type="button"
                    className="btn ghost"
                    style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}
                    onClick={() => navigate(decoratePath(`/system-release/${attempt.target_release_id}`))}
                  >
                    <span>{t('systemRelease.restoreReleaseType')}</span>
                    <span>{attempt.target_version}</span>
                    <span>
                      {attempt.release_group === 'models'
                        ? t('systemRelease.groupModels')
                        : attempt.release_group === 'infra'
                          ? t('systemRelease.groupInfraAlt')
                          : t('systemRelease.groupAll')}
                    </span>
                    <span>{getRestorePhaseLabel(attempt.phase)}</span>
                    <span className={`release-status ${statusClass}`}>{getDisplayStatus(status)}</span>
                    <span>{attempt.completed_at || attempt.created_at}</span>
                  </button>
                );
              })}
            </div>
          </div>
        ) : null}

        {Array.isArray(detail.devices) && detail.devices.length > 0 ? (
          <div className="release-devices" style={{ margin: '20px 0' }}>
            <header className="release-diff-header">
              <h4>{t('systemRelease.devicesTitle')}</h4>
            </header>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '12px' }}>
              {machineGroups.map((machine) => {
                const machineLabel = machine.deviceName
                  ? `${machine.deviceName} (${machine.ip}:${machine.port})`
                  : `${machine.ip}:${machine.port}`;
                const machineStatus = getMachineStatus(
                  machine.targets,
                  detail.restore_info?.release_group || detail.release_group,
                  detail.restore_info?.status,
                );
                return (
                  <div
                    key={machine.key}
                    style={{
                      border: '1px solid #f0f0f0',
                      borderRadius: '10px',
                      padding: '12px 16px',
                      background: '#fafafa',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '12px' }}>
                      <span style={{ fontWeight: 600, fontSize: '14px' }}>{machineLabel}</span>
                      <span className={`release-status ${getDeviceStatusClass(machineStatus)}`}>
                        {getDisplayStatus(machineStatus)}
                      </span>
                    </div>
                    <div
                      style={{
                        display: 'grid',
                        gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
                        gap: '12px',
                      }}
                    >
                      {['models', 'infra'].map((group) => (
                        <div key={group}>
                          {renderDeviceTargetPanel(machine.targets[group], group)}
                        </div>
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        <div className="release-diff">
          <header className="release-diff-header">
            <div>
              <h4 style={{ marginBottom: 4 }}>
                {t(detail.status === 'WAIT' ? 'systemRelease.draftVersionDiff' : 'systemRelease.versionDiff')}
              </h4>
              <div className="release-diff-hint">
                {t('systemRelease.diffLoadedProgress', {
                  loaded: loadedDiffFileCount,
                  total: totalDiffFileCount,
                })}
              </div>
            </div>
            <div className="release-diff-totals">
              <span className="release-diff-total-chip">
                {t('systemRelease.changedFilesSummary')}: {totalDiffStats.files_changed}
              </span>
              <span className="release-diff-total-chip is-add">
                +{totalDiffStats.insertions}
              </span>
              <span className="release-diff-total-chip is-delete">
                -{totalDiffStats.deletions}
              </span>
            </div>
          </header>
          <div className="release-diff-shell">
            <aside className="release-diff-sidebar">
              <div className="release-diff-sidebar-summary">
                {diffGroups.map((group) => {
                  const title = getReleaseGroupTitle(group.release_group);
                  const subtitle = group.previous_version
                    ? t('systemRelease.diffGroupVersionVsPrevious', {
                        current: group.current_version,
                        previous: group.previous_version,
                      })
                    : t('systemRelease.diffGroupVersionVsInitial', {
                        current: group.current_version === 'draft'
                          ? t('systemRelease.draftLabel')
                          : group.current_version,
                      });

                  return (
                    <section key={group.release_group} className="release-diff-sidebar-group">
                      <header className="release-diff-sidebar-group-header">
                        <div>
                          <h5>{title}</h5>
                          <p>{subtitle}</p>
                        </div>
                        <span className="release-diff-sidebar-group-count">
                          {group.total_files}
                        </span>
                      </header>
                      <div className="release-diff-sidebar-group-stats">
                        <span>M {group.changeBuckets.modified}</span>
                        <span>D {group.changeBuckets.deleted}</span>
                        <span>A {group.changeBuckets.added}</span>
                        <span>{t('systemRelease.diffFileCount', { count: group.total_files })}</span>
                      </div>
                      <div className="release-diff-sidebar-files">
                        {group.loadedFiles.map((file) => {
                          const codeStyle = getFileChangeCodeStyle(file.change_type);
                          return (
                            <button
                              key={file.diff_anchor_id}
                              type="button"
                              className="release-diff-file-item"
                              onClick={() => handleJumpToDiff(file.diff_anchor_id)}
                              title={t('systemRelease.jumpToDiff')}
                            >
                              <span className="release-diff-file-item-path">{file.file_path}</span>
                              <span
                                className="release-diff-file-item-code"
                                style={{
                                  borderColor: codeStyle.borderColor,
                                  background: codeStyle.background,
                                  color: codeStyle.color,
                                }}
                              >
                                {getFileChangeCode(file.change_type)}
                              </span>
                            </button>
                          );
                        })}
                        {!group.loadedFiles.length && group.total_files === 0 ? (
                          <div className="release-diff-file-item-empty">
                            {t('systemRelease.noFileChangesInCategory')}
                          </div>
                        ) : null}
                      </div>
                    </section>
                  );
                })}
              </div>
            </aside>

            <section className="release-diff-content-panel">
              {diffState.loading ? (
                <div className="release-diff-placeholder">{t('common.loading')}</div>
              ) : null}

              {!diffState.loading && diffState.error && !deferredDiffFiles.length ? (
                <div className="release-error-content">{diffState.error.message}</div>
              ) : null}

              {!diffState.loading && diffState.initialized && !hasDiffFiles ? (
                <div className="release-diff-placeholder">{t('systemRelease.noFileChangesInCategory')}</div>
              ) : null}

              {deferredDiffFiles.length > 0 ? (
                <DiffViewer
                  files={deferredDiffFiles}
                  viewType="split"
                  loading={false}
                  collapsedLineThreshold={RELEASE_DETAIL_COLLAPSED_LINE_THRESHOLD}
                  getFileAnchorId={(file) => file.diff_anchor_id}
                  focusedFileAnchorId={focusedDiffAnchorId}
                />
              ) : null}

              {diffState.error && deferredDiffFiles.length > 0 ? (
                <div className="release-error-content">{diffState.error.message}</div>
              ) : null}

              <div ref={loadMoreSentinelRef} className="release-diff-loader">
                {diffState.loadingMore ? t('systemRelease.diffLoadingMore') : null}
                {!diffState.hasMore && hasDiffFiles && loadedDiffFileCount >= totalDiffFileCount
                  ? t('systemRelease.diffLoadedAll')
                  : null}
              </div>
            </section>
          </div>
        </div>
      </div>
      <FloatButton.BackTop
        visibilityHeight={320}
        icon={<UpOutlined />}
        style={{ right: 24, bottom: 24 }}
      />
    </div>
  );
}

export default ReleaseDetailPage;
