import React, { useEffect, useState, useCallback } from 'react';
import { Modal, Input, message, Card, Row, Col, Tag, Empty, Spin, Tooltip } from 'antd';
import { PlusOutlined, EditOutlined, DeleteOutlined, CheckCircleOutlined, CloseCircleOutlined, ReloadOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { fetchConnections, createConnection, updateConnection, deleteConnection, refreshConnectionStatus } from '@/services/connection';

/**
 * 连接管理组件
 * 功能：
 * 1. 卡片式连接展示（名称、IP、端口、状态、备注）
 * 2. 新增 / 编辑 / 删除连接
 * 3. 在线状态展示（Active / Inactive）
 * 4. 实时搜索（无需查询按钮）
 * 5. 健康检查由后端定时触发，前端仅展示状态
 */
function ConnectionManage() {
  const { t } = useTranslation();

  // 列表数据
  const [loading, setLoading] = useState(false);
  const [dataSource, setDataSource] = useState([]);
  const [keyword, setKeyword] = useState('');

  // 表单弹窗
  const [modalOpen, setModalOpen] = useState(false);
  const [editRecord, setEditRecord] = useState(null); // null = 新增模式
  const [formValues, setFormValues] = useState({ name: '', ip: '', port: '', token: '', remark: '' });
  const [submitting, setSubmitting] = useState(false);
  const [refreshingMap, setRefreshingMap] = useState({});

  /**
   * 加载连接列表
   */
  const loadConnections = useCallback(async () => {
    setLoading(true);
    try {
      const resp = await fetchConnections({ keyword, page: 1, pageSize: 100 });
      setDataSource(resp.items || []);
    } finally {
      setLoading(false);
    }
  }, [keyword]);

  useEffect(() => {
    // 使用防抖，避免频繁请求
    const timer = setTimeout(() => {
      loadConnections();
    }, 300);
    return () => clearTimeout(timer);
  }, [loadConnections]);

  /**
   * 打开新增弹窗
   */
  const handleAdd = () => {
    setEditRecord(null);
    setFormValues({ name: '', ip: '', port: '', token: '', remark: '' });
    setModalOpen(true);
  };

  /**
   * 打开编辑弹窗
   * @param {Object} record - 连接记录
   */
  const handleEdit = (record) => {
    setEditRecord(record);
    setFormValues({
      name: record.name || '',
      ip: record.ip || '',
      port: String(record.port || ''),
      token: record.token || '',
      remark: record.remark || '',
    });
    setModalOpen(true);
  };

  /**
   * 确认删除
   * @param {Object} record - 连接记录
   */
  const handleDelete = (record) => {
    Modal.confirm({
      title: t('connectionManage.deleteConfirm'),
      content: t('connectionManage.deleteConfirmMessage', {
        name: record.name || record.ip,
        ip: record.ip,
        port: record.port,
      }),
      okType: 'danger',
      onOk: async () => {
        try {
          await deleteConnection({ id: record.id });
          message.success(t('connectionManage.deleteSuccess'));
          loadConnections();
        } catch (err) {
          message.error(t('connectionManage.deleteFailed', { message: err.message }));
        }
      },
    });
  };

  /**
   * 手动刷新设备在线状态
   */
  const handleRefreshStatus = async (record) => {
    if (!record?.id) return;
    setRefreshingMap((prev) => ({ ...prev, [record.id]: true }));
    try {
      await refreshConnectionStatus(record.id);
      message.success(t('connectionManage.refreshSuccess'));
      await loadConnections();
    } catch (err) {
      message.error(
        t('connectionManage.refreshFailed', {
          message: err.message || 'unknown',
        }),
      );
    } finally {
      setRefreshingMap((prev) => {
        const next = { ...prev };
        delete next[record.id];
        return next;
      });
    }
  };

  /**
   * 校验表单基础字段
   * @returns {boolean} 是否通过校验
   */
  const validateForm = () => {
    const { ip, port, token } = formValues;
    if (!ip.trim()) {
      message.warning(t('connectionManage.ipRequired'));
      return false;
    }
    const portNum = Number(port);
    if (!port || Number.isNaN(portNum) || portNum < 1 || portNum > 65535) {
      message.warning(t('connectionManage.portInvalid'));
      return false;
    }
    if (!token.trim()) {
      message.warning(t('connectionManage.tokenRequired'));
      return false;
    }
    return true;
  };

  /**
   * 提交表单（新增 or 编辑）
   */
  const handleSubmit = async () => {
    if (!validateForm()) return;
    setSubmitting(true);
    try {
      const payload = {
        name: formValues.name.trim() || undefined,
        ip: formValues.ip.trim(),
        port: Number(formValues.port),
        token: formValues.token.trim(),
        remark: formValues.remark.trim() || undefined,
      };
      if (editRecord) {
        await updateConnection({ id: editRecord.id, ...payload });
        message.success(t('connectionManage.updateSuccess'));
      } else {
        await createConnection(payload);
        message.success(t('connectionManage.createSuccess'));
      }
      setModalOpen(false);
      loadConnections();
    } catch (err) {
      message.error(err.message || t('connectionManage.saveFailed'));
    } finally {
      setSubmitting(false);
    }
  };

  /**
   * 渲染在线状态标签
   * @param {string} status - active / inactive
   */
  const renderStatus = (status) => {
    const normalizedStatus = String(status || '').toLowerCase();
    const isActive = normalizedStatus === 'active';
    
    if (isActive) {
      return (
        <Tag icon={<CheckCircleOutlined />} color="success">
          {t('connectionManage.statusActive')}
        </Tag>
      );
    }
    
    return (
      <Tag icon={<CloseCircleOutlined />} color="error">
        {t('connectionManage.statusInactive')}
      </Tag>
    );
  };

  /**
   * 渲染单个连接卡片
   */
  const renderMaskedToken = (value = '') => {
    if (!value) return '—';
    return '*'.repeat(Math.max(value.length, 6));
  };

  const renderConnectionCard = (record) => (
    <Card
      key={record.id}
      hoverable
      style={{
        borderRadius: '8px',
        boxShadow: '0 2px 8px rgba(0,0,0,0.08)',
        transition: 'all 0.3s',
      }}
      bodyStyle={{ padding: '20px' }}
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
        {/* 标题和状态 */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: '12px' }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: '16px', fontWeight: 600, color: '#262626', marginBottom: '4px' }}>
              {record.name || record.ip}
            </div>
            {record.name && (
              <div style={{ fontSize: '13px', color: '#8c8c8c' }}>
                {record.ip}:{record.port}
              </div>
            )}
            {!record.name && (
              <div style={{ fontSize: '13px', color: '#8c8c8c' }}>
                {t('connectionManage.port')}: {record.port}
              </div>
            )}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            {renderStatus(record.status)}
            <Tooltip title={t('connectionManage.refreshStatus')}>
              <button
                type="button"
                aria-label={t('connectionManage.refreshStatus')}
                style={{
                  border: 'none',
                  background: 'transparent',
                  padding: 4,
                  borderRadius: '50%',
                  color: '#555',
                  cursor: refreshingMap[record.id] ? 'not-allowed' : 'pointer',
                }}
                disabled={!!refreshingMap[record.id]}
                onClick={() => handleRefreshStatus(record)}
              >
                <ReloadOutlined spin={!!refreshingMap[record.id]} />
              </button>
            </Tooltip>
          </div>
        </div>

        <div style={{ fontSize: '13px', color: '#8c8c8c' }}>
          {t('connectionManage.token')}: {renderMaskedToken(record.token)}
        </div>

        {/* 版本信息 */}
        {(record.client_version || record.config_version) && (
          <div style={{ fontSize: '13px', color: '#595959', display: 'flex', gap: '12px' }}>
            {record.client_version && (
              <span>
                <span style={{ color: '#8c8c8c' }}>应用版本: </span>
                <span style={{ fontFamily: 'monospace' }}>{record.client_version}</span>
              </span>
            )}
            {record.config_version && (
              <span>
                <span style={{ color: '#8c8c8c' }}>配置版本: </span>
                <span style={{ fontFamily: 'monospace' }}>{record.config_version}</span>
              </span>
            )}
          </div>
        )}

        {/* 备注 */}
        {record.remark && (
          <div style={{ fontSize: '13px', color: '#595959', lineHeight: '1.6' }}>
            {record.remark}
          </div>
        )}

        {/* 操作按钮 */}
        <div style={{ display: 'flex', gap: '8px', marginTop: '8px', paddingTop: '12px', borderTop: '1px solid #f0f0f0' }}>
          <button
            type="button"
            className="btn btn-sm"
            style={{
              flex: 1,
              background: '#e8f4fd',
              color: 'var(--primary)',
              padding: '6px 12px',
              fontSize: '13px',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              gap: '4px',
            }}
            onClick={() => handleEdit(record)}
          >
            <EditOutlined />
            {t('common.edit')}
          </button>
          <button
            type="button"
            className="btn btn-sm"
            style={{
              flex: 1,
              background: '#fef3f2',
              color: 'var(--danger)',
              padding: '6px 12px',
              fontSize: '13px',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              gap: '4px',
            }}
            onClick={() => handleDelete(record)}
          >
            <DeleteOutlined />
            {t('common.delete')}
          </button>
        </div>
      </div>
    </Card>
  );

  return (
    <>
      {/* 顶部操作栏 */}
      <div style={{ display: 'flex', gap: '12px', marginBottom: '24px', alignItems: 'center' }}>
        <Input
          placeholder={t('connectionManage.searchPlaceholder')}
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
          style={{ width: 320 }}
          allowClear
          size="large"
        />
        <button
          type="button"
          className="btn primary"
          style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: '6px' }}
          onClick={handleAdd}
        >
          <PlusOutlined />
          {t('connectionManage.addConnection')}
        </button>
      </div>

      {/* 连接卡片网格 */}
      <Spin spinning={loading}>
        {dataSource.length === 0 && !loading ? (
          <Empty
            description={keyword ? t('connectionManage.noSearchResults') : t('connectionManage.noConnections')}
            style={{ marginTop: '60px' }}
          />
        ) : (
          <Row gutter={[16, 16]}>
            {dataSource.map((record) => (
              <Col key={record.id} xs={24} sm={12} lg={8} xl={6}>
                {renderConnectionCard(record)}
              </Col>
            ))}
          </Row>
        )}
      </Spin>

      {/* 新增 / 编辑弹窗 */}
      <Modal
        title={editRecord ? t('connectionManage.editConnection') : t('connectionManage.addConnection')}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        confirmLoading={submitting}
        onOk={handleSubmit}
        okText={t('common.confirm')}
        cancelText={t('common.cancel')}
        width={480}
        destroyOnClose
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: '14px', padding: '8px 0' }}>
          <div>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '13px', color: '#666' }}>
              {t('connectionManage.name')}
            </label>
            <Input
              value={formValues.name}
              placeholder={t('connectionManage.namePlaceholder')}
              onChange={(e) => setFormValues({ ...formValues, name: e.target.value })}
            />
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '13px', color: '#666' }}>
              {t('connectionManage.ip')} *
            </label>
            <Input
              value={formValues.ip}
              placeholder={t('connectionManage.ipPlaceholder')}
              onChange={(e) => setFormValues({ ...formValues, ip: e.target.value })}
            />
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '13px', color: '#666' }}>
              {t('connectionManage.port')} *
            </label>
            <Input
              value={formValues.port}
              placeholder={t('connectionManage.portPlaceholder')}
              onChange={(e) => setFormValues({ ...formValues, port: e.target.value })}
            />
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '13px', color: '#666' }}>
              {t('connectionManage.token')} *
            </label>
            <Input.Password
              value={formValues.token}
              placeholder={t('connectionManage.tokenPlaceholder')}
              onChange={(e) => setFormValues({ ...formValues, token: e.target.value })}
              visibilityToggle={false}
            />
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '13px', color: '#666' }}>
              {t('connectionManage.remark')}
            </label>
            <Input
              value={formValues.remark}
              placeholder={t('connectionManage.remarkPlaceholder')}
              onChange={(e) => setFormValues({ ...formValues, remark: e.target.value })}
            />
          </div>
        </div>
      </Modal>
    </>
  );
}

export default ConnectionManage;
