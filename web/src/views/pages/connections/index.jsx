import React, { useEffect, useState } from 'react';
import { Input, Modal, Pagination } from 'antd';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import {
  fetchConnections,
  setConnected,
  testPort,
  createConnection,
  updateConnection,
  deleteConnection,
  testToken,
} from '@/services/connection';
import { logout } from '@/services/auth';

/**
 * 连接管理页面
 * 功能：
 * 1. 显示可用连接列表（使用网格布局）
 * 2. 支持选择连接
 * 3. 支持连接、编辑、新增连接
 * 4. 支持测试端口和Token
 * 对应原型：pages/views/connection-manage.html
 */
function ConnectionsPage() {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [connections, setConnections] = useState([]);
  const [selectedConnection, setSelectedConnection] = useState(null);
  const [editingConnection, setEditingConnection] = useState(null);
  const [modalVisible, setModalVisible] = useState(false);
  const [modalForm, setModalForm] = useState({
    ip: '',
    port: '',
    git: '',
    token: '',
    remark: '',
  });
  const [testingPort, setTestingPort] = useState(false);
  const [testingToken, setTestingToken] = useState(false);
  const [keyword, setKeyword] = useState('');
  const [currentPage, setCurrentPage] = useState(1);
  const pageSize = 5;
  const [totalConnections, setTotalConnections] = useState(0);

  // 简单的 IP 和端口校验
  const validateIpAndPort = () => {
    const { ip, port } = modalForm;

    if (!ip || !port) {
      Modal.warning({
        title: t('connections.hint'),
        content: t('connections.fillRequired'),
      });
      return false;
    }

    // 基础 IP 校验（支持 IPv4，简单校验即可）
    const ipv4Pattern = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/;
    if (!ipv4Pattern.test(ip)) {
      Modal.warning({
        title: t('connections.invalidIp'),
        content: t('connections.invalidIpMessage'),
      });
      return false;
    }

    const portNumber = Number(port);
    if (!Number.isInteger(portNumber) || portNumber <= 0 || portNumber > 65535) {
      Modal.warning({
        title: t('connections.invalidPort'),
        content: t('connections.invalidPortMessage'),
      });
      return false;
    }

    return true;
  };

  const getErrorMessage = (error, fallbackMessage) => {
    if (!error) return fallbackMessage;

    // 优先取原始 message
    const rawMessage = error.message || error?.response?.statusText || '';

    // 请求超时（前端 10s 超时）统一提示为“连接超时”
    if (rawMessage && /timeout/i.test(rawMessage)) {
      return t('connections.connectionTimeout');
    }

    // 网络错误（例如后端服务未启动）统一中文提示
    if (rawMessage && /network error/i.test(rawMessage)) {
      return t('connections.networkError');
    }

    // 若有明确的 message，直接使用
    if (rawMessage) {
      return rawMessage;
    }

    // 兼容后端 AppError 结构
    const responseData = error.response?.data;
    if (responseData?.error?.message) return responseData.error.message;
    if (typeof responseData === 'string') return responseData;
    return fallbackMessage;
  };

  /**
   * 加载连接列表数据
   */
  const loadConnections = async (options = {}) => {
    const { keyword: keywordOption, page } = options;
    const searchKeyword =
      typeof keywordOption === 'string' ? keywordOption : keyword;
    const targetPage = page || currentPage || 1;

    setLoading(true);
    try {
      const response = await fetchConnections({
        keyword: searchKeyword || undefined,
        page: targetPage,
        pageSize,
      });
      // 转换数据格式以匹配旧版本
      const formattedConnections = (response.items || []).map((conn) => ({
        id: conn.id,
        ip: conn.ip,
        port: String(conn.port),
        version: conn.version || '—',
        git: conn.gitRepo || '',
        token: '',
        status: conn.status || 'offline',
        remark: conn.remark || '',
      }));
      setConnections(formattedConnections);
      setCurrentPage(response.page || targetPage);
      setTotalConnections(response.total || formattedConnections.length);
    } catch (error) {
      Modal.error({
        title: t('connections.loadFailed'),
        content: getErrorMessage(error, t('connections.loadFailedMessage')),
      });
    } finally {
      setLoading(false);
    }
  };

  // 组件挂载时加载数据
  useEffect(() => {
    loadConnections({ page: 1 });
  }, []);

  /**
   * 处理连接项点击
   */
  const handleConnectionClick = (connection) => {
    setSelectedConnection(connection);
  };

  /**
   * 处理连接按钮点击
   */
  const handleConnect = () => {
    if (!selectedConnection) return;

    // 保存连接信息到sessionStorage
    sessionStorage.setItem('connectedIP', selectedConnection.ip);
    sessionStorage.setItem('connectedPort', selectedConnection.port);
    sessionStorage.setItem('connectedVersion', selectedConnection.version);
    sessionStorage.setItem('connectedStatus', selectedConnection.status);
    // 保存连接 ID，供规则配置等页面使用
    if (selectedConnection.id != null) {
      sessionStorage.setItem('connectedId', String(selectedConnection.id));
    }

    // 设置连接
    setConnected(selectedConnection.ip);

    // 显示连接成功消息
    Modal.success({
      title: t('connections.connectSuccess'),
      content: ``,
      onOk: () => {
        navigate('/rule-manage', { replace: true });
      },
    });
  };

  /**
   * 处理新增按钮点击
   */
  const handleAdd = () => {
    setEditingConnection(null);
    setModalForm({ ip: '', port: '', git: '', token: '', remark: '' });
    setModalVisible(true);
  };

  /**
   * 处理编辑按钮点击
   */
  const handleEdit = () => {
    if (!selectedConnection) return;
    setEditingConnection(selectedConnection);
    setModalForm({
      ip: selectedConnection.ip,
      port: selectedConnection.port,
      git: selectedConnection.git || '',
      token: selectedConnection.token || '',
      remark: selectedConnection.remark || '',
    });
    setModalVisible(true);
  };

  /**
   * 处理测试端口（调用后端 /api/connections/test-port 接口）
   */
  const handleTestPort = async () => {
    if (!validateIpAndPort()) {
      return;
    }

    const { ip, port } = modalForm;

    setTestingPort(true);
    try {
      const response = await testPort({ ip, port: Number(port) });
      const { success, message } = response || {};
      if (success) {
        Modal.success({
          title: t('connections.testPortSuccess'),
          content: message || `${ip}:${port} 可以正常连接`,
        });
      } else {
        Modal.error({
          title: t('connections.testPortFailed'),
          content:
            message ||
            `无法连接到 ${ip}:${port}\n\n请检查：\n• IP 地址和端口是否正确\n• 服务是否已启动\n• 防火墙设置是否允许连接`,
        });
      }
    } catch (error) {
      Modal.error({
        title: t('connections.testPortError'),
        content: getErrorMessage(error, `测试端口 ${ip}:${port} 时发生异常，请稍后重试`),
      });
    } finally {
      setTestingPort(false);
    }
  };

  /**
   * 处理测试Token（调用后端 /api/connections/test-token 接口）
   */
  const handleTestToken = async () => {
    const { git, token } = modalForm;
    if (!git || !token) {
      Modal.warning({
        title: t('connections.tokenError'),
        content: t('connections.tokenErrorMessage'),
      });
      return;
    }

    setTestingToken(true);
    try {
      const response = await testToken({ gitRepo: git, gitToken: token });
      const { valid, message } = response || {};
      if (valid) {
        Modal.success({
          title: t('connections.testTokenSuccess'),
          content: message || `具有对 ${git} 的访问权限`,
        });
      } else {
        Modal.error({
          title: t('connections.testTokenFailed'),
          content:
            message ||
            `Token 无效或没有访问权限\n\n请检查：\n• Token 是否正确\n• Token 是否已过期\n• 仓库地址是否正确`,
        });
      }
    } catch (error) {
      Modal.error({
        title: t('connections.testTokenError'),
        content: getErrorMessage(error, '验证 Token 时发生异常，请稍后重试'),
      });
    } finally {
      setTestingToken(false);
    }
  };

  /**
   * 处理保存连接（区分新增/编辑，调用后端接口后刷新列表）
   */
  const handleSave = async () => {
    const { ip, port, git, token, remark } = modalForm;
    if (!validateIpAndPort()) {
      return;
    }

    try {
      if (editingConnection) {
        await updateConnection({
          id: editingConnection.id,
          ip,
          port: Number(port),
          gitRepo: git,
          gitToken: token,
          remark,
        });
      } else {
        await createConnection({
          ip,
          port: Number(port),
          gitRepo: git,
          gitToken: token,
          remark,
        });
      }

      // 保存成功后重新加载列表，确保与后端一致
      await loadConnections();

      // 若当前有选中项且处于编辑状态，尝试保持选中
      if (editingConnection) {
        setSelectedConnection((prevSelected) => {
          if (!prevSelected || prevSelected.id !== editingConnection.id) {
            return prevSelected;
          }
          return {
            ...prevSelected,
            ip,
            port: String(port),
            git,
            token,
            remark,
          };
        });
      }
    } catch (error) {
      Modal.error({
        title: editingConnection ? t('connections.updateFailed') : t('connections.createFailed'),
        content: getErrorMessage(
          error,
          editingConnection ? '更新连接时发生错误，请稍后重试' : '创建连接时发生错误，请稍后重试',
        ),
      });
    } finally {
      setModalVisible(false);
      setEditingConnection(null);
    }
  };

  /**
   * 处理退出登录
   */
  const handleLogout = () => {
    Modal.confirm({
      title: t('connections.logoutButton'),
      content: t('connections.logoutConfirmMessage'),
      okText: t('connections.logoutButton'),
      cancelText: t('common.cancel'),
      onOk: () => {
        logout();
        navigate('/login', { replace: true });
      },
    });
  };

  return (
    <div
      className="connection-page-wrapper"
      style={{ display: 'flex', justifyContent: 'center' }}
    >
      <div
        className="connection-container"
        style={{ maxWidth: 1600, width: '100%', margin: '0 auto' }}
      >
      <div className="connection-panel" style={{ margin: '0 auto' }}>
        <div className="panel-header">
          <h1 className="panel-title">连接管理</h1>
          <button type="button" className="logout-btn" onClick={handleLogout}>
            退出登录
          </button>
        </div>

        <div className="panel-body">
          <div className="connections-list">
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                marginBottom: 12,
              }}
            >
              <div style={{ fontWeight: 500 }}>连接列表</div>
              <Input.Search
                allowClear
                size="small"
                placeholder="搜索 IP 或 Git 仓库"
                value={keyword}
                onChange={(event) => setKeyword(event.target.value)}
                onSearch={(value) => {
                  setKeyword(value);
                  loadConnections({ keyword: value, page: 1 });
                }}
                style={{ width: 260 }}
              />
            </div>
            <div
              className="list-header"
              style={{
                display: 'grid',
                gridTemplateColumns: '110px 50px 60px 150px 60px 150px',
                columnGap: 8,
              }}
            >
              <div style={{ whiteSpace: 'nowrap' }}>IP地址</div>
              <div style={{ whiteSpace: 'nowrap' }}>端口</div>
              <div style={{ whiteSpace: 'nowrap' }}>版本</div>
              <div style={{ whiteSpace: 'nowrap' }}>Git仓库地址</div>
              <div style={{ whiteSpace: 'nowrap' }}>状态</div>
              <div style={{ whiteSpace: 'nowrap' }}>备注</div>
            </div>
            <div id="connections-container">
              {loading ? (
                <div style={{ padding: '40px', textAlign: 'center' }}>加载中...</div>
              ) : (
                connections.map((conn) => (
                    <div
                      key={conn.id}
                      className={`connection-item ${selectedConnection?.id === conn.id ? 'selected' : ''}`}
                      onClick={() => handleConnectionClick(conn)}
                      style={{
                        display: 'grid',
                        gridTemplateColumns: '110px 50px 60px 150px 60px 150px',
                        columnGap: 8,
                      }}
                    >
                      <div
                        style={{
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                        title={conn.ip}
                      >
                        {conn.ip}
                      </div>
                      <div>{conn.port}</div>
                      <div>{conn.version}</div>
                      <div
                        style={{
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                        title={conn.git || '—'}
                      >
                        {conn.git || '—'}
                      </div>
                      <div>
                        <span className={`status-badge status-${conn.status}`}>
                          {conn.status === 'online' ? t('connections.online') : t('connections.offline')}
                        </span>
                      </div>
                      <div
                        style={{
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                        title={conn.remark || '—'}
                      >
                        {conn.remark || '—'}
                      </div>
                    </div>
                  ))
              )}
            </div>
            <div
              style={{
                padding: '8px 12px 0',
                marginTop: 8,
                display: 'flex',
                justifyContent: 'flex-end',
              }}
            >
              <Pagination
                size="small"
                current={currentPage}
                pageSize={pageSize}
                total={totalConnections}
                showSizeChanger={false}
                onChange={(page) => {
                  setCurrentPage(page);
                  loadConnections({ page });
                }}
              />
            </div>
          </div>

          <div className="actions-panel">
            <button
              type="button"
              className="action-btn btn-connect"
              disabled={!selectedConnection}
              onClick={handleConnect}
            >
              连接
            </button>
            <button
              type="button"
              className="action-btn btn-add"
              onClick={handleAdd}
            >
              新增
            </button>
            <button
              type="button"
              className="action-btn btn-edit"
              disabled={!selectedConnection}
              onClick={handleEdit}
            >
              编辑
            </button>
            <button
              type="button"
              className="action-btn btn-delete"
              disabled={!selectedConnection}
              style={{ border: '1px solid #ff4d4f', color: '#ff4d4f' }}
              onClick={() => {
                if (!selectedConnection) return;
                Modal.confirm({
                  title: '删除连接',
                  content: `确定要删除连接 ${selectedConnection.ip}:${selectedConnection.port} 吗？`,
                  okText: t('connections.delete'),
                  cancelText: t('common.cancel'),
                  onOk: async () => {
                    try {
                      await deleteConnection({ id: selectedConnection.id });
                      await loadConnections();
                      setSelectedConnection(null);
                    } catch (error) {
                      Modal.error({
                        title: t('connections.deleteFailed'),
                        content: getErrorMessage(error, '删除连接时发生错误，请稍后重试'),
                      });
                    }
                  },
                });
              }}
            >
              删除
            </button>
          </div>
        </div>
      </div>

      {/* 新增/编辑连接 Modal */}
      <Modal
        title={editingConnection ? t('connections.editConnection') : t('connections.addConnection')}
        open={modalVisible}
        onCancel={() => {
          setModalVisible(false);
          setEditingConnection(null);
        }}
        footer={[
          <button
            key="cancel"
            type="button"
            className="btn-cancel"
            onClick={() => {
              setModalVisible(false);
              setEditingConnection(null);
            }}
          >
            取消
          </button>,
          <button key="confirm" type="button" className="btn-confirm" onClick={handleSave}>
            确定
          </button>,
        ]}
        width={500}
        className="connection-modal"
      >
        <div className="modal-body">
          <div className="form-group">
            <label>IP地址 *</label>
            <Input
              value={modalForm.ip}
              onChange={(e) => setModalForm({ ...modalForm, ip: e.target.value })}
              placeholder="例如：192.168.1.1"
            />
          </div>
          <div className="form-group">
            <label>端口 *</label>
            <div className="input-with-test">
              <Input
                type="number"
                value={modalForm.port}
                onChange={(e) => setModalForm({ ...modalForm, port: e.target.value })}
                placeholder="例如：8001"
              />
              <button
                type="button"
                className="btn-test"
                onClick={handleTestPort}
                disabled={testingPort}
              >
                {testingPort ? t('connections.testing') : t('connections.test')}
              </button>
            </div>
          </div>
          <div className="form-group">
            <label>Git仓库地址</label>
            <Input
              value={modalForm.git}
              onChange={(e) => setModalForm({ ...modalForm, git: e.target.value })}
              placeholder="例如：github.com/repo"
            />
          </div>
          <div className="form-group">
            <label>Git Token</label>
            <div className="input-with-test">
              <Input.Password
                value={modalForm.token}
                onChange={(e) => setModalForm({ ...modalForm, token: e.target.value })}
                placeholder="请输入Git Token"
              />
              <button
                type="button"
                className="btn-test"
                onClick={handleTestToken}
                disabled={testingToken}
              >
                {testingToken ? t('connections.testing') : t('connections.test')}
              </button>
            </div>
          </div>
          <div className="form-group">
            <label>备注</label>
            <Input.TextArea
              value={modalForm.remark}
              onChange={(e) => setModalForm({ ...modalForm, remark: e.target.value })}
              placeholder="可以填一些环境说明、用途等备注信息"
              autoSize={{ minRows: 2, maxRows: 4 }}
            />
          </div>
        </div>
      </Modal>
      </div>
    </div>
  );
}

export default ConnectionsPage;
