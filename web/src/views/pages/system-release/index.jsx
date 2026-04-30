import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DatePicker, Input, Modal, Table, Select, Checkbox, Spin, Radio } from 'antd';
import { useNavigate } from 'react-router-dom';
import { fetchReleases, publishRelease, validateRelease } from '@/services/release';
import { fetchOnlineConnections } from '@/services/connection';
import ValidateResultModal from '@/components/ValidateResultModal';

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
  }, [page, pageSize]);

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
      const result = await validateRelease(releaseRecord.id);
      const details = Array.isArray(result.details) ? result.details : [];
      setValidateResult({
        filename: result.filename || `版本 ${releaseRecord.version}`,
        valid: result.valid !== false,
        message: result.message || (details.length > 0 ? details.join('\n') : ''),
        details,
        type: result.type || t('validation.releasePackage'),
      });
      setValidateModalVisible(true);
    } catch (error) {
      setValidateResult({
        filename: `版本 ${releaseRecord.version}`,
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
      const connections = await fetchOnlineConnections();
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
        return (
          <>
            <button
              type="button"
              className="link-btn release-detail-btn"
              onClick={() => {
                navigate(`/system-release/${releaseRecord.id}`);
              }}
            >
              {t('systemRelease.detail')}
            </button>
            <button
              type="button"
              className="link-btn release-prepublish-btn"
              onClick={() => navigate(`/system-release/${releaseRecord.id}/prepublish`)}
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
            size="small"
            className="data-table release-table"
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
