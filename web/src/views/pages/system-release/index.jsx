import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DatePicker, Dropdown, Input, Modal, Table, Select, Checkbox, Spin, Radio } from 'antd';
import { useNavigate } from 'react-router-dom';
import { fetchReleases, publishRelease, restoreRelease, validateRelease } from '@/services/release';
import { fetchOnlineConnections } from '@/services/connection';
import { useSystem } from '@/contexts/SystemContext';
import {
  confirmProjectArchiveImport,
  buildProjectFolderArchive,
  downloadBlob,
  exportProjectArchive,
  importProjectArchive,
} from '@/services/project';
import ValidateResultModal from '@/components/ValidateResultModal';
import ProjectImportResult from '@/views/components/ProjectImportResult';

const IMPORTABLE_FOLDER_NAMES = ['conf', 'connectors', 'models', 'topology'];

/**
 * 系统发布列表页面
 * 功能：
 * 1. 显示发布列表
 * 2. 支持查看发布详情
 * 对应原型：pages/views/system-release/release-list.html
 */
function SystemReleasePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { currentSystem, decoratePath } = useSystem();
  const [loading, setLoading] = useState(false);
  const [dataSource, setDataSource] = useState([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  
  // 查询表单状态
  const [searchForm, setSearchForm] = useState({
    version: '',
    pipeline: '',
    status: '',
    updatedAtRange: [],
  });

  // 弹窗状态
  const [validateModalVisible, setValidateModalVisible] = useState(false);
  const [publishModalVisible, setPublishModalVisible] = useState(false);
  const [currentRelease, setCurrentRelease] = useState(null);
  const [validateResult, setValidateResult] = useState(null);

  // 发布弹窗：在线机器列表 & 选中机器
  const [onlineConnections, setOnlineConnections] = useState([]);
  const [loadingConnections, setLoadingConnections] = useState(false);
  const [selectedConnectionIds, setSelectedConnectionIds] = useState([]);
  const [publishNote, setPublishNote] = useState('');
  const [publishReleaseGroup, setPublishReleaseGroup] = useState('models');
  const [importingArchive, setImportingArchive] = useState(false);
  const [exportingArchive, setExportingArchive] = useState(false);
  const [restoringReleaseId, setRestoringReleaseId] = useState(null);

  const getAvailablePublishGroups = (releaseRecord) => {
    const status = String(releaseRecord?.status || '').toUpperCase();
    const releaseGroup = releaseRecord?.releaseGroup || 'draft';

    if (status === 'RUNNING') {
      return [];
    }

    if (status === 'PASS') {
      if (releaseGroup === 'all') return [];
      if (releaseGroup === 'models') return ['infra'];
      if (releaseGroup === 'infra') return ['models'];
      return ['models', 'infra'];
    }

    if (releaseGroup === 'models') return ['models', 'infra'];
    if (releaseGroup === 'infra') return ['infra', 'models'];
    return ['models', 'infra'];
  };
  const availablePublishGroups = getAvailablePublishGroups(currentRelease);

  const getErrorMessage = (error, fallback) => {
    const responseData = error?.response?.data || error?.data || error?.responseData;
    const backendError = responseData?.error;
    const details = backendError?.details || backendError?.detail;

    if (typeof responseData === 'string' && responseData.trim()) {
      return responseData;
    }
    if (typeof details === 'string' && details.trim()) {
      return details;
    }
    if (backendError?.message) {
      return backendError.message;
    }
    return error?.message || fallback;
  };

  const buildArchiveImportFailureResult = (message, sourceLabel = '') => ({
    summary: {
      rules_deleted: 0,
      rules_imported: 0,
      knowledge_deleted: 0,
      knowledge_imported: 0,
      imported_dirs: [],
      retained_dirs: [],
      rule_breakdown: [],
      warnings: [],
      failed_files: 1,
      source_dir: sourceLabel,
      models_root: '',
      infra_root: '',
    },
    validation: {
      passed: false,
      message,
    },
  });

  const showArchiveImportResultModal = (title, result, showPaths = true) => {
    Modal.info({
      title,
      width: 760,
      icon: null,
      okText: t('common.confirm'),
      content: <ProjectImportResult result={result} showPaths={showPaths} system={currentSystem} />,
    });
  };

  /**
   * 翻译阶段名称
   */
  const translateStageLabel = (label) => {
    const stageMap = {
      '校验': t('systemRelease.stageValidate'),
      '同步git': t('systemRelease.stageSyncGit'),
      '打包': t('systemRelease.stagePackage'),
      '发布': t('systemRelease.stagePublish'),
      '草稿': t('systemRelease.stageDraft'),
    };
    return stageMap[label] || label;
  };

  /**
   * 加载发布列表数据
   */
  const loadReleases = async () => {
    setLoading(true);
    try {
      const response = await fetchReleases({
        ...searchForm,
        page,
        pageSize,
        system: currentSystem,
      });
      setDataSource(response.items || []);
      setTotal(response.total || 0);
    } finally {
      setLoading(false);
    }
  };

  // 组件挂载时加载数据
  useEffect(() => {
    loadReleases();
  }, [currentSystem, page, pageSize]);

  /**
   * 处理查询按钮点击
   */
  const handleSearch = () => {
    setPage(1); // 重置到第一页
    loadReleases();
  };

  /**
   * 处理重置按钮点击
   */
  const handleReset = () => {
    setSearchForm({
      version: '',
      pipeline: '',
      status: '',
      updatedAtRange: [],
    });
    setPage(1);
  };

  /**
   * 处理分页变化
   */
  const handlePageChange = (newPage, newPageSize) => {
    setPage(newPage);
    if (newPageSize !== pageSize) {
      setPageSize(newPageSize);
    }
  };

  /**
   * 处理校验按钮点击
   */
  const handleValidate = async (releaseRecord) => {
    setCurrentRelease(releaseRecord);
    try {
      const result = await validateRelease(releaseRecord.id, currentSystem);
      const details = Array.isArray(result.details) ? result.details : [];
      setValidateResult({
        filename: result.filename || `版本 ${releaseRecord.version}`,
        subjectLabel: t('systemRelease.versionNumber'),
        subjectValue: releaseRecord.version || '—',
        valid: result.valid !== false,
        message: result.message || (details.length > 0 ? details.join('\n') : ''),
        details,
        type: result.type || t('validation.releasePackage'),
      });
      setValidateModalVisible(true);
    } catch (error) {
      setValidateResult({
        filename: `版本 ${releaseRecord.version}`,
        subjectLabel: t('systemRelease.versionNumber'),
        subjectValue: releaseRecord.version || '—',
        valid: false,
        message: error.message || t('systemRelease.validateFailedMessage'),
        details: [],
        type: t('validation.releasePackage'),
      });
      setValidateModalVisible(true);
    }
  };

  /**
   * 处理发布按钮点击
   * 同时加载在线机器列表供用户多选
   */
  const handlePublish = async (releaseRecord) => {
    if (!releaseRecord?.sandboxReady) {
      Modal.warning({
        title: t('sandbox.publishBlockedTitle'),
        content: t('sandbox.publishBlocked'),
      });
      return;
    }

    const availableGroups = getAvailablePublishGroups(releaseRecord);
    if (availableGroups.length === 0) {
      Modal.warning({
        title: t('systemRelease.publishWarning'),
        content: t('systemRelease.statusPublishedAll'),
      });
      return;
    }

    setCurrentRelease(releaseRecord);
    setSelectedConnectionIds([]);
    setPublishNote('');
    setPublishReleaseGroup(availableGroups[0] || 'models');
    setPublishModalVisible(true);
    // 异步加载在线机器
    setLoadingConnections(true);
    try {
      const connections = await fetchOnlineConnections(currentSystem);
      setOnlineConnections(connections);
    } finally {
      setLoadingConnections(false);
    }
  };

  /**
   * 确认发布
   */
  const handleConfirmPublish = async () => {
    if (!currentRelease) return;
    if (availablePublishGroups.length === 0) {
      Modal.warning({
        title: t('systemRelease.publishWarning'),
        content: t('systemRelease.statusPublishedAll'),
      });
      return;
    }

    // 校验：必须至少选择一台机器
    if (selectedConnectionIds.length === 0) {
      Modal.warning({
        title: t('systemRelease.publishWarning'),
        content: t('systemRelease.selectAtLeastOneMachine'),
      });
      return;
    }

    try {
      // 将选中的在线机器 ID 传给后端
      const result = await publishRelease(
        currentRelease.id,
        publishReleaseGroup,
        selectedConnectionIds,
        publishNote,
        currentSystem,
      );
      Modal.success({
        title: t('systemRelease.publishSuccess'),
        content:
          result?.message || t('systemRelease.publishSuccessMessage', { version: currentRelease.version }),
      });

      setPublishModalVisible(false);
      setCurrentRelease(null);
      setSelectedConnectionIds([]);
      setPublishNote('');
      setPublishReleaseGroup('models');

      // 刷新列表
      loadReleases();
    } catch (error) {
      Modal.error({
        title: t('systemRelease.publishFailed'),
        content: getErrorMessage(error, t('systemRelease.publishFailedMessage')),
      });
    }
  };

  const isSupportedArchive = (fileName) => {
    const lower = String(fileName || '').toLowerCase();
    return lower.endsWith('.tar') || lower.endsWith('.tar.gz') || lower.endsWith('.tgz') || lower.endsWith('.zip');
  };

  const handleImportArchiveFile = async (file, sourceLabel = file?.name) => {
    if (!file) return;

    if (!isSupportedArchive(file.name)) {
      Modal.warning({
        title: t('systemRelease.importArchiveInvalidTitle'),
        content: t('systemRelease.importArchiveInvalidMessage'),
      });
      return;
    }

    setImportingArchive(true);
    try {
      const preview = await importProjectArchive(file, currentSystem);
      Modal.confirm({
        title: t('systemRelease.importArchiveConfirmTitle'),
        content: (
          <div style={{ lineHeight: 1.7, marginTop: 8 }}>
            <p>{t('systemRelease.importArchivePreviewMessage', { file: sourceLabel })}</p>
            <ProjectImportResult result={preview} showPaths={false} system={currentSystem} />
          </div>
        ),
        width: 720,
        okText: t('common.confirm'),
        cancelText: t('common.cancel'),
        onOk: async () => {
          setImportingArchive(true);
          try {
            const result = await confirmProjectArchiveImport(preview.import_id, currentSystem);
            showArchiveImportResultModal(t('systemRelease.importArchiveSuccessTitle'), result);
            loadReleases();
          } catch (error) {
            showArchiveImportResultModal(
              t('systemRelease.importArchiveFailedTitle'),
              buildArchiveImportFailureResult(
                error?.message || t('systemRelease.importArchiveFailedMessage'),
                sourceLabel,
              ),
              false,
            );
          } finally {
            setImportingArchive(false);
          }
        },
      });
    } catch (error) {
      showArchiveImportResultModal(
        t('systemRelease.importArchiveFailedTitle'),
        buildArchiveImportFailureResult(
          error?.message || t('systemRelease.importArchiveFailedMessage'),
          sourceLabel,
        ),
        false,
      );
    } finally {
      setImportingArchive(false);
    }
  };

  const handleImportArchive = async (event) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    await handleImportArchiveFile(file, file?.name);
  };

  const handleImportFolder = async (event) => {
    const files = Array.from(event.target.files || []);
    event.target.value = '';
    if (files.length === 0) return;

    const folderLabel = files[0]?.webkitRelativePath?.split('/')[0] || '所选文件夹';
    await handleImportFolderEntries(files, folderLabel);
  };

  const collectDirectoryFiles = async (directoryHandle) => {
    const entries = [];
    const selectedFolderIsImportable = IMPORTABLE_FOLDER_NAMES.includes(directoryHandle.name);
    const walk = async (handle, pathParts, insideImportableFolder) => {
      for await (const [name, entry] of handle.entries()) {
        const nextPathParts = [...pathParts, name];

        if (entry.kind === 'file') {
          if (insideImportableFolder) {
            entries.push({
              file: await entry.getFile(),
              relativePath: nextPathParts.join('/'),
            });
          }
          continue;
        }

        if (entry.kind === 'directory') {
          const isDirectImportableFolder =
            pathParts.length === 1 && IMPORTABLE_FOLDER_NAMES.includes(name);
          if (insideImportableFolder || isDirectImportableFolder) {
            await walk(entry, nextPathParts, insideImportableFolder || isDirectImportableFolder);
          }
        }
      }
    };

    await walk(directoryHandle, [directoryHandle.name], selectedFolderIsImportable);
    return entries;
  };

  const handleImportFolderEntries = async (entries, folderLabel) => {
    try {
      const archive = buildProjectFolderArchive(entries);
      await handleImportArchiveFile(archive, `文件夹：${folderLabel}`);
    } catch (error) {
      showArchiveImportResultModal(
        t('systemRelease.importArchiveFailedTitle'),
        buildArchiveImportFailureResult(
          error?.message || t('systemRelease.importArchiveFailedMessage'),
          `文件夹：${folderLabel}`,
        ),
        false,
      );
    }
  };

  const handleSelectImportFolder = async () => {
    // File System Access API 不会触发“上传整个文件夹”的浏览器确认，
    // 前端只读取核心目录下的文件并生成归档。
    if (typeof window.showDirectoryPicker === 'function') {
      try {
        const directoryHandle = await window.showDirectoryPicker({ mode: 'read' });
        const entries = await collectDirectoryFiles(directoryHandle);
        await handleImportFolderEntries(entries, directoryHandle.name || '所选文件夹');
      } catch (error) {
        if (error?.name === 'AbortError') return;
        showArchiveImportResultModal(
          t('systemRelease.importArchiveFailedTitle'),
          buildArchiveImportFailureResult(
            error?.message || t('systemRelease.importArchiveFailedMessage'),
            '所选文件夹',
          ),
          false,
        );
      }
      return;
    }

    // 兼容不支持 File System Access API 的浏览器。
    document.getElementById('project-folder-import')?.click();
  };

  const handleRestore = (releaseRecord) => {
    Modal.confirm({
      title: t('systemRelease.restoreConfirmTitle'),
      content: (
        <div style={{ lineHeight: 1.7 }}>
          <p>{t('systemRelease.restoreConfirmMessage', { version: releaseRecord.version })}</p>
          <p style={{ color: '#b42318', marginBottom: 0 }}>
            {t('systemRelease.restoreConfirmWarning')}
          </p>
        </div>
      ),
      width: 560,
      okType: 'danger',
      okText: t('systemRelease.restore'),
      cancelText: t('common.cancel'),
      onOk: async () => {
        setRestoringReleaseId(releaseRecord.id);
        try {
          const result = await restoreRelease(releaseRecord.id, currentSystem);
          Modal.success({
            title: t('systemRelease.restoreSuccessTitle'),
            content: result?.message || t('systemRelease.restoreSuccessMessage'),
          });
          await loadReleases();
        } catch (error) {
          Modal.error({
            title: t('systemRelease.restoreFailedTitle'),
            content: error?.message || t('systemRelease.restoreFailedMessage'),
          });
        } finally {
          setRestoringReleaseId(null);
        }
      },
    });
  };

  const importConfigMenuItems = [
    {
      key: 'archive',
      label: t('systemRelease.importArchive'),
      onClick: () => document.getElementById('project-archive-import')?.click(),
    },
    {
      key: 'folder',
      label: t('systemRelease.importFolder'),
      onClick: handleSelectImportFolder,
    },
  ];

  const handleExportArchive = async () => {
    setExportingArchive(true);
    try {
      const { blob, fileName } = await exportProjectArchive(currentSystem);
      downloadBlob(blob, fileName);
    } catch (error) {
      Modal.error({
        title: t('systemRelease.exportArchiveFailedTitle'),
        content: error?.message || t('systemRelease.exportArchiveFailedMessage'),
      });
    } finally {
      setExportingArchive(false);
    }
  };

  const renderReleaseGroupTag = (releaseGroup) => {
    if (!releaseGroup || releaseGroup === 'draft') return null;
    const label =
      releaseGroup === 'models'
        ? t('systemRelease.groupModels')
        : releaseGroup === 'infra'
          ? t('systemRelease.groupInfra')
          : t('systemRelease.groupAll');
    return (
      <span className="release-status" style={{ marginLeft: 8 }}>
        {label}
      </span>
    );
  };

  const renderSystemTag = (system) => (
    <span className="release-status" style={{ marginLeft: 0 }}>
      {t(`navigation.system.${system || 'wparse'}`)}
    </span>
  );

  const getReleaseStatusMeta = (record) => {
    const normalizedStatus = String(record?.status || '').toUpperCase();
    const releaseGroup = record?.releaseGroup || 'draft';

    if (normalizedStatus === 'RUNNING') {
      return { className: 'release-status is-running', text: t('systemRelease.statusRunning') };
    }
    if (normalizedStatus === 'FAIL' || normalizedStatus === 'PARTIAL_FAIL') {
      return { className: 'release-status is-fail', text: t('systemRelease.statusFailed') };
    }
    if (normalizedStatus === 'PASS') {
      if (releaseGroup === 'models') {
        return { className: 'release-status is-pass', text: t('systemRelease.statusPublishedModels') };
      }
      if (releaseGroup === 'infra') {
        return { className: 'release-status is-pass', text: t('systemRelease.statusPublishedInfra') };
      }
      if (releaseGroup === 'all') {
        return { className: 'release-status is-pass', text: t('systemRelease.statusPublishedAll') };
      }
    }
    return { className: 'release-status is-wait', text: t('systemRelease.statusDraft') };
  };

  const renderStageIcon = (stage, index) => {
    const status = String(stage?.status || '').toLowerCase();
    let stageClass = 'stage-icon';
    let stageIcon = '>>';

    if (status === 'pass') {
      stageClass = 'stage-icon is-pass';
      stageIcon = '✓';
    } else if (status === 'fail') {
      stageClass = 'stage-icon is-fail';
      stageIcon = '✗';
    } else if (status === 'running') {
      stageClass = 'stage-icon is-running';
      stageIcon = '…';
    }

    return (
      <span key={index} className={stageClass} title={translateStageLabel(stage.label || '')}>
        {stageIcon}
      </span>
    );
  };

  const columns = [
    {
      title: t('systemRelease.status'),
      dataIndex: 'status',
      key: 'status',
      // 不设置固定宽度，让表格自动分配，与旧版本一致
      render: (_, record) => {
        const meta = getReleaseStatusMeta(record);
        return <span className={meta.className}>{meta.text}</span>;
      },
    },
    {
      title: t('systemRelease.system'),
      dataIndex: 'system',
      key: 'system',
      render: (system) => renderSystemTag(system),
    },
    {
      title: t('systemRelease.versionNumber'),
      dataIndex: 'version',
      key: 'version',
      render: (version, record) => (
        <span>
          {version}
          {renderReleaseGroupTag(record.releaseGroup)}
        </span>
      ),
    },
    {
      title: t('systemRelease.remark'),
      dataIndex: 'pipeline',
      key: 'pipeline',
      // pipeline 列不设置宽度，占据剩余空间
      render: (pipeline) => pipeline || '—',
    },
    {
      title: t('systemRelease.stages'),
      dataIndex: 'stages',
      key: 'stages',
      // 不设置固定宽度，让表格自动分配
      render: (stages) => {
        if (!stages || stages.length === 0) {
          return <div className="release-stages release-stages--empty">—</div>;
        }
        return (
          <div className="release-stages">
            {stages.map(renderStageIcon)}
          </div>
        );
      },
    },
    {
      title: t('systemRelease.updateTime'),
      dataIndex: 'updatedAt',
      key: 'updatedAt',
      // 后端返回 ISO 字符串，直接渲染或根据需要格式化
    },
    {
      title: t('systemRelease.publishTime'),
      dataIndex: 'publishedAt',
      key: 'publishedAt',
      render: (value) => value || '—',
    },
    {
      title: t('systemRelease.operator'),
      dataIndex: 'owner',
      key: 'owner',
    },
    {
      title: t('systemRelease.actions'),
      key: 'action',
      // 不设置固定宽度，让表格自动分配
      render: (_, releaseRecord) => {
        const statusUpper = String(releaseRecord.status || '').toUpperCase();
        const availableGroups = getAvailablePublishGroups(releaseRecord);
        const publishHidden = statusUpper === 'PASS' && availableGroups.length === 0;
        const publishDisabled =
          !(releaseRecord.sandboxReady ?? false) || statusUpper === 'RUNNING';
        const restoreDisabled = restoringReleaseId === releaseRecord.id;
        return (
          <>
            <button
              type="button"
              className="link-btn release-detail-btn"
              onClick={() => navigate(decoratePath(`/system-release/${releaseRecord.id}`))}
            >
              {t('systemRelease.detail')}
            </button>
            <button
              type="button"
              className="link-btn release-prepublish-btn"
              onClick={() =>
                navigate(decoratePath(`/system-release/${releaseRecord.id}/prepublish`))
              }
            >
              {statusUpper === 'PASS'
                ? t('sandbox.prepublishDetail')
                : t('sandbox.startSandbox')}
            </button>
            {statusUpper !== 'RUNNING' && (
              <>
                <button
                  type="button"
                  className="link-btn release-validate-btn"
                  onClick={() => handleValidate(releaseRecord)}
                >
                  {t('systemRelease.validate')}
                </button>
                {!publishHidden && (
                  <button
                    type="button"
                    className="link-btn release-publish-btn"
                    onClick={() => handlePublish(releaseRecord)}
                    disabled={publishDisabled}
                    title={publishDisabled ? t('sandbox.publishBlocked') : undefined}
                    style={
                      publishDisabled
                        ? { opacity: 0.4, cursor: 'not-allowed' }
                        : undefined
                    }
                  >
                    {t('systemRelease.publish')}
                  </button>
                )}
              </>
            )}
            {statusUpper === 'PASS' && (
              <button
                type="button"
                className="link-btn release-restore-btn"
                onClick={() => handleRestore(releaseRecord)}
                disabled={restoreDisabled}
                title={t('systemRelease.restoreButtonHint')}
              >
                {restoreDisabled ? t('systemRelease.restoring') : t('systemRelease.restore')}
              </button>
            )}
          </>
        );
      },
    },
  ];

  return (
    <div className="panel is-visible">
      {/* 页面头部 */}
      <header className="panel-header">
        <h2>{t('systemRelease.title')}</h2>
      </header>
      
      {/* 页面主体 */}
      <section className="panel-body release-body">
        {/* 查询表单 */}
        <form className="form-grid release-query">
          <div className="form-row">
            <label htmlFor="release-version">{t('systemRelease.version')}</label>
            <Input
              id="release-version"
              placeholder={t('systemRelease.versionPlaceholder')}
              size="middle"
              value={searchForm.version}
              onChange={(e) => setSearchForm({ ...searchForm, version: e.target.value })}
            />
          </div>
          <div className="form-row">
            <label htmlFor="release-pipeline">{t('systemRelease.release')}</label>
            <Input
              id="release-pipeline"
              placeholder={t('systemRelease.releasePlaceholder')}
              size="middle"
              value={searchForm.pipeline}
              onChange={(e) => setSearchForm({ ...searchForm, pipeline: e.target.value })}
            />
          </div>
          <div className="form-row">
            <label htmlFor="release-status">{t('systemRelease.status')}</label>
            <Select
              id="release-status"
              allowClear
              placeholder={t('systemRelease.statusPlaceholder')}
              size="middle"
              value={searchForm.status || undefined}
              onChange={(value) =>
                setSearchForm({ ...searchForm, status: value || '' })
              }
              options={[
                { value: 'WAIT', label: t('systemRelease.statusWait') },
                { value: 'INIT', label: t('systemRelease.statusInit') },
                { value: 'PASS', label: t('systemRelease.statusPass') },
                { value: 'FAIL', label: t('systemRelease.statusFail') },
              ]}
              style={{ width: '100%' }}
            />
          </div>
          <div className="form-row">
            <label htmlFor="release-updated-at">{t('systemRelease.updateTime')}</label>
            <DatePicker.RangePicker
              id="release-updated-at"
              size="middle"
              style={{ width: '100%' }}
              value={searchForm.updatedAtRange}
              onChange={(dates) =>
                setSearchForm({ ...searchForm, updatedAtRange: dates || [] })
              }
            />
          </div>
          <div className="form-row form-row-actions">
            <button type="button" className="btn primary" onClick={handleSearch}>
              {t('systemRelease.query')}
            </button>
            <button type="button" className="btn ghost" onClick={handleReset}>
              {t('systemRelease.reset')}
            </button>
          </div>
        </form>

        {/* 发布记录 */}
        <div className="release-main">
          <header className="release-list-header">
            <h3>{t('systemRelease.releaseRecords')}</h3>
            <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
              <span className="release-list-hint">{t('systemRelease.recentRecords', { count: total })}</span>
              <input
                id="project-archive-import"
                type="file"
                style={{ display: 'none' }}
                onChange={handleImportArchive}
              />
              <input
                id="project-folder-import"
                type="file"
                webkitdirectory="true"
                directory="true"
                multiple
                style={{ display: 'none' }}
                onChange={handleImportFolder}
              />
              <Dropdown
                menu={{ items: importConfigMenuItems }}
                trigger={['click']}
                disabled={importingArchive}
              >
                <button
                  type="button"
                  className="btn ghost"
                  disabled={importingArchive}
                  aria-haspopup="menu"
                >
                  {importingArchive ? t('systemRelease.importingConfig') : t('systemRelease.importConfig')}
                </button>
              </Dropdown>
              <button
                type="button"
                className="btn ghost"
                disabled={exportingArchive}
                onClick={handleExportArchive}
              >
                {exportingArchive ? t('systemRelease.exportingArchive') : t('systemRelease.exportArchive')}
              </button>
            </div>
          </header>
          
          {/* 发布列表表格 */}
          <Table 
            rowKey="id" 
            loading={loading} 
            columns={columns} 
            dataSource={dataSource} 
            pagination={{
              current: page,
              pageSize: pageSize,
              total: total,
              onChange: handlePageChange,
              showSizeChanger: false,
              showQuickJumper: true,
              showTotal: (total) => t('systemRelease.total', { count: total }),
              position: ['bottomCenter'],
            }}
            className="release-table"
          />
        </div>
      </section>

      {/* 校验结果弹窗 */}
      <ValidateResultModal
        open={validateModalVisible}
        onClose={() => {
          setValidateModalVisible(false);
          setValidateResult(null);
        }}
        result={validateResult}
      />

      <Modal
        title={t('systemRelease.confirmPublish')}
        open={publishModalVisible}
        onCancel={() => {
          setPublishModalVisible(false);
          setCurrentRelease(null);
          setSelectedConnectionIds([]);
          setPublishNote('');
          setPublishReleaseGroup('models');
        }}
        footer={[
          <button
            key="cancel"
            type="button"
            className="btn ghost"
            onClick={() => {
              setPublishModalVisible(false);
              setCurrentRelease(null);
              setSelectedConnectionIds([]);
              setPublishNote('');
              setPublishReleaseGroup('models');
            }}
          >
            {t('common.cancel')}
          </button>,
          <button
            key="confirm"
            type="button"
            className="btn primary"
            onClick={handleConfirmPublish}
          >
            {t('common.confirm')}
          </button>,
        ]}
        width={520}
      >
        <p style={{ margin: '0 0 12px', fontSize: '14px', lineHeight: '1.6' }}>
          {t('systemRelease.confirmPublishMessage', { version: currentRelease?.version })}
        </p>
        <div style={{ marginBottom: '16px' }}>
          <div style={{ marginBottom: '8px', fontSize: '13px', color: '#666', fontWeight: 500 }}>
            {t('systemRelease.releaseGroup')}
          </div>
          <Radio.Group
            value={publishReleaseGroup}
            onChange={(e) => setPublishReleaseGroup(e.target.value)}
          >
            {availablePublishGroups.includes('models') && (
              <Radio value="models">{t('systemRelease.groupModels')}</Radio>
            )}
            {availablePublishGroups.includes('infra') && (
              <Radio value="infra">{t('systemRelease.groupInfraAlt')}</Radio>
            )}
          </Radio.Group>
        </div>
        <div style={{ marginBottom: '8px', fontSize: '13px', color: '#666', fontWeight: 500 }}>
          {t('systemRelease.selectTargetMachines')}
        </div>
        {loadingConnections ? (
          <div style={{ textAlign: 'center', padding: '20px 0' }}>
            <Spin size="small" />
          </div>
        ) : onlineConnections.length === 0 ? (
          <div style={{ color: '#999', fontSize: '13px', padding: '8px 0' }}>
            {t('systemRelease.noOnlineMachines')}
          </div>
        ) : (
          <div style={{ maxHeight: '240px', overflowY: 'auto', border: '1px solid #f0f0f0', borderRadius: '8px', padding: '8px 12px' }}>
            {onlineConnections.map((conn) => (
              <div key={conn.id} style={{ padding: '6px 0', borderBottom: '1px solid #f9f9f9' }}>
                <Checkbox
                  checked={selectedConnectionIds.includes(conn.id)}
                  onChange={(e) => {
                    if (e.target.checked) {
                      setSelectedConnectionIds((prev) => [...prev, conn.id]);
                    } else {
                      setSelectedConnectionIds((prev) => prev.filter((id) => id !== conn.id));
                    }
                  }}
                >
                  <span style={{ fontSize: '13px' }}>
                    {conn.name ? `${conn.name} (${conn.ip}:${conn.port})` : `${conn.ip}:${conn.port}`}
                  </span>
                </Checkbox>
              </div>
            ))}
          </div>
        )}
        {selectedConnectionIds.length > 0 && (
          <div style={{ marginTop: '8px', fontSize: '12px', color: '#275efe' }}>
            {t('systemRelease.selectedMachines', { count: selectedConnectionIds.length })}
          </div>
        )}
        <div style={{ marginTop: '16px' }}>
          <div style={{ fontSize: '13px', color: '#666', marginBottom: '6px' }}>
            {t('systemRelease.publishNoteLabel')}
          </div>
          <Input.TextArea
            rows={3}
            maxLength={200}
            value={publishNote}
            placeholder={t('systemRelease.publishNotePlaceholder')}
            onChange={(e) => setPublishNote(e.target.value)}
          />
        </div>
      </Modal>
    </div>
  );
}

export default SystemReleasePage;
