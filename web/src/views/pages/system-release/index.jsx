import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DatePicker, Input, Modal, Table, Select, Checkbox, Spin } from 'antd';
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

    setCurrentRelease(releaseRecord);
    setSelectedConnectionIds([]);
    setPublishNote('');
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
      const result = await publishRelease(currentRelease.id, selectedConnectionIds, publishNote);
      Modal.success({
        title: t('systemRelease.publishSuccess'),
        content:
          result?.message || t('systemRelease.publishSuccessMessage', { version: currentRelease.version }),
      });

      setPublishModalVisible(false);
      setCurrentRelease(null);
      setSelectedConnectionIds([]);
      setPublishNote('');

      // 刷新列表
      loadReleases();
    } catch (error) {
      Modal.error({
        title: t('systemRelease.publishFailed'),
        content: error.message || t('systemRelease.publishFailedMessage'),
      });
    }
  };

  const columns = [
    {
      title: t('systemRelease.status'),
      dataIndex: 'status',
      key: 'status',
      // 不设置固定宽度，让表格自动分配，与旧版本一致
      render: (status) => {
        // 根据状态设置样式，使用与旧版本一致的类名
        const normalizedStatus = String(status || '').toUpperCase();
        const statusClassMap = {
          WAIT: 'release-status is-wait',
          PASS: 'release-status is-pass',
          FAIL: 'release-status is-fail',
          RUNNING: 'release-status is-running',
          PARTIAL_FAIL: 'release-status is-running',
          INIT: 'release-status is-wait',
        };
        const statusTextMap = {
          WAIT: '草稿',
          PASS: '已发布',
          FAIL: 'FAIL',
          RUNNING: 'RUNNING',
          PARTIAL_FAIL: 'PARTIAL_FAIL',
          INIT: 'INIT',
        };
        const statusClass = statusClassMap[normalizedStatus] || 'release-status';
        const statusText = statusTextMap[normalizedStatus] || normalizedStatus || '—';
        return <span className={statusClass}>{statusText}</span>;
      },
    },
    {
      title: t('systemRelease.versionNumber'),
      dataIndex: 'version',
      key: 'version',
      // 不设置固定宽度，让表格自动分配
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
        // 渲染阶段图标，使用与旧版本一致的类名
        if (!stages || stages.length === 0) {
          return <div className="release-stages release-stages--empty">—</div>;
        }
        let hasFailedBefore = false;
        return (
          <div className="release-stages">
            {stages.map((stage, index) => {
              const status = String(stage?.status || '').toLowerCase();
              let stageClass = 'stage-icon';
              let stageIcon = '>>';

              if (hasFailedBefore) {
                // 之前已经有失败阶段，后续统一展示待执行 >>
                stageClass = 'stage-icon';
                stageIcon = '>>';
              } else if (status === 'pass') {
                stageClass = 'stage-icon is-pass';
                stageIcon = '✓';
              } else if (status === 'fail') {
                stageClass = 'stage-icon is-fail';
                stageIcon = '✗';
                hasFailedBefore = true;
              } else {
                // pending 等待执行，使用白色圆形 >>
                stageClass = 'stage-icon';
                stageIcon = '>>';
              }

              return (
                <span key={index} className={stageClass} title={translateStageLabel(stage.label || '')}>
                  {stageIcon}
                </span>
              );
            })}
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
        const publishDisabled = !(releaseRecord.sandboxReady ?? false);
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
            {statusUpper === 'WAIT' && (
              <>
                <button
                  type="button"
                  className="link-btn release-validate-btn"
                  onClick={() => handleValidate(releaseRecord)}
                >
                  {t('systemRelease.validate')}
                </button>
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
            <span className="release-list-hint">{t('systemRelease.recentRecords', { count: total })}</span>
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

      {/* 发布确认弹窗 */}
      <Modal
        title={t('systemRelease.confirmPublish')}
        open={publishModalVisible}
        onCancel={() => {
          setPublishModalVisible(false);
          setCurrentRelease(null);
          setSelectedConnectionIds([]);
          setPublishNote('');
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
