import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Table, Modal, Form, Input, Select, message } from 'antd';
import { fetchUsers, createUser, updateUser, updateUserStatus, resetUserPassword, changeUserPassword, deleteUser } from '@/services/user';
import { fetchOperationLogs } from '@/services/operation_log';
import ConnectionManage from './ConnectionManage';

/**
 * 系统管理页面
 * 功能：
 * 1. 用户管理（查询、编辑、禁用、删除等）
 * 2. 操作日志查看
 * 3. 帮助中心
 * 对应原型：pages/views/system-manage/user-list.html
 */
function SystemManagePage() {
  const { t } = useTranslation();
  const [activeKey, setActiveKey] = useState('connections');
  const [loading, setLoading] = useState(false);
  const [dataSource, setDataSource] = useState([]);
  const [searchForm, setSearchForm] = useState({ username: '', role: '', status: '' });
  
  // 用户弹窗相关状态
  const [userModalVisible, setUserModalVisible] = useState(false);
  const [userModalMode, setUserModalMode] = useState('create'); // create / edit
  const [currentUser, setCurrentUser] = useState(null);
  const [userForm] = Form.useForm();
  
  // 修改密码弹窗相关状态
  const [passwordModalVisible, setPasswordModalVisible] = useState(false);
  const [passwordForm] = Form.useForm();
  
  // 操作日志相关状态
  const [logLoading, setLogLoading] = useState(false);
  const [logDataSource, setLogDataSource] = useState([]);
  const [logSearchForm, setLogSearchForm] = useState({
    operator: '',
    operation: '',
    startDate: '',
    endDate: '',
  });

  /**
   * 加载用户列表数据
   */
  const loadUsers = async () => {
    setLoading(true);
    try {
      // searchForm.username 映射到 fetchUsers 的 keyword 参数
      const response = await fetchUsers({
        keyword: searchForm.username,
        role: searchForm.role,
        status: searchForm.status,
      });
      setDataSource(response.items || []);
    } catch (error) {
      message.error(error.message || '加载用户列表失败');
    } finally {
      setLoading(false);
    }
  };

  /**
   * 加载操作日志数据
   */
  const loadLogs = async () => {
    setLogLoading(true);
    try {
      const response = await fetchOperationLogs({
        operator: logSearchForm.operator,
        operation: logSearchForm.operation,
        startDate: logSearchForm.startDate,
        endDate: logSearchForm.endDate,
      });
      setLogDataSource(response.items || []);
    } catch (error) {
      message.error(error.message || '加载操作日志失败');
    } finally {
      setLogLoading(false);
    }
  };

  // 当切换到对应页面时加载数据
  useEffect(() => {
    if (activeKey === 'users') {
      loadUsers();
    } else if (activeKey === 'logs') {
      loadLogs();
    }
    // connections 由 ConnectionManage 组件自行管理数据加载
  }, [activeKey]);

  /**
   * 处理搜索按钮点击
   */
  const handleSearch = () => {
    loadUsers();
  };

  /**
   * 打开新增用户弹窗
   */
  const handleAddUser = () => {
    setUserModalMode('create');
    setCurrentUser(null);
    userForm.resetFields();
    setUserModalVisible(true);
  };

  /**
   * 打开编辑用户弹窗
   */
  const handleEditUser = (record) => {
    setUserModalMode('edit');
    setCurrentUser(record);
    userForm.setFieldsValue({
      username: record.username,
      displayName: record.displayName,
      email: record.email,
      role: record.role,
      remark: record.remark,
    });
    setUserModalVisible(true);
  };

  /**
   * 提交用户表单（新增或编辑）
   */
  const handleUserSubmit = async () => {
    try {
      const values = await userForm.validateFields();
      
      if (userModalMode === 'create') {
        await createUser({
          username: values.username,
          password: values.password,
          displayName: values.displayName,
          email: values.email,
          role: values.role,
          remark: values.remark,
        });
        message.success('用户创建成功');
      } else {
        await updateUser(currentUser.id, {
          displayName: values.displayName,
          email: values.email,
          role: values.role,
          remark: values.remark,
        });
        message.success('用户信息更新成功');
      }
      
      setUserModalVisible(false);
      loadUsers();
    } catch (error) {
      // 表单验证失败或 API 调用失败
      console.error('用户操作失败:', error);
    }
  };

  /**
   * 打开修改密码弹窗
   */
  const handleChangePassword = (record) => {
    setCurrentUser(record);
    passwordForm.resetFields();
    setPasswordModalVisible(true);
  };

  /**
   * 提交修改密码表单
   */
  const handlePasswordSubmit = async () => {
    try {
      const values = await passwordForm.validateFields();
      
      await changeUserPassword(currentUser.id, {
        oldPassword: values.oldPassword,
        newPassword: values.newPassword,
        confirmPassword: values.confirmPassword,
      });
      
      message.success('密码修改成功');
      setPasswordModalVisible(false);
    } catch (error) {
      console.error('修改密码失败:', error);
    }
  };

  /**
   * 处理重置按钮点击
   * 清空搜索表单
   */
  const handleReset = () => {
    setSearchForm({ username: '', role: '', status: '' });
  };

  /**
   * 处理用户操作
   * @param {string} action - 操作类型（edit/reset-password/change-password/disable/enable/delete）
   * @param {Object} userRecord - 用户记录
   */
  const handleAction = (action, userRecord) => {
    const actionMap = {
      edit: () => handleEditUser(userRecord),
      'change-password': () => handleChangePassword(userRecord),
      'reset-password': () => {
        Modal.confirm({
          title: t('systemManage.resetPassword'),
          content: `确定要重置用户 "${userRecord.username}" 的密码吗？系统将生成一个随机密码。`,
          onOk: async () => {
            const result = await resetUserPassword(userRecord.id);
            Modal.info({
              title: '密码重置成功',
              content: (
                <div>
                  <p>用户 "{userRecord.username}" 的新密码为：</p>
                  <p style={{ fontSize: '16px', fontWeight: 'bold', color: '#1890ff', padding: '10px', background: '#f0f0f0', borderRadius: '4px', fontFamily: 'monospace' }}>
                    {result.new_password}
                  </p>
                  <p style={{ color: '#ff4d4f', marginTop: '10px' }}>请妥善保管此密码，关闭后将无法再次查看！</p>
                </div>
              ),
            });
            loadUsers();
          },
        });
      },
      disable: () => {
        Modal.confirm({
          title: '禁用用户',
          content: `确定要禁用用户 "${userRecord.username}" 吗？禁用后该用户将无法登录系统。`,
          onOk: async () => {
            await updateUserStatus(userRecord.id, 'inactive');
            message.success(`用户 "${userRecord.username}" 已禁用`);
            loadUsers();
          },
        });
      },
      enable: () => {
        Modal.confirm({
          title: '启用用户',
          content: `确定要启用用户 "${userRecord.username}" 吗？`,
          onOk: async () => {
            await updateUserStatus(userRecord.id, 'active');
            message.success(`用户 "${userRecord.username}" 已启用`);
            loadUsers();
          },
        });
      },
      delete: () => {
        Modal.confirm({
          title: '删除用户',
          content: `确定要删除用户 "${userRecord.username}" 吗？此操作不可恢复！`,
          okType: 'danger',
          onOk: async () => {
            await deleteUser(userRecord.id);
            message.success(`用户 "${userRecord.username}" 已删除`);
            loadUsers();
          },
        });
      },
    };
    actionMap[action]?.();
  };

  /**
   * 获取角色徽章
   * @param {string} role - 角色
   * @returns {JSX.Element} 徽章元素
   */
  const getRoleBadge = (role) => {
    const roleMap = {
      admin: { label: t('systemManage.admin'), className: 'badge--primary' },
      operator: { label: t('systemManage.operator'), className: 'badge--info' },
      viewer: { label: t('systemManage.viewer'), className: 'badge--secondary' },
    };
    const config = roleMap[role] || { label: role, className: 'badge--secondary' };
    return <span className={`badge ${config.className}`}>{config.label}</span>;
  };

  /**
   * 获取状态标签
   * @param {string} status - 状态
   * @returns {JSX.Element} 标签元素
   */
  const getStatusTag = (status) => {
    const statusMap = {
      active: { label: t('systemManage.enable'), className: 'status-tag--success' },
      inactive: { label: t('systemManage.disable'), className: 'status-tag--inactive' },
    };
    const config = statusMap[status] || { label: status, className: 'status-tag--inactive' };
    return <span className={`status-tag ${config.className}`}>{config.label}</span>;
  };

  const columns = [
    { title: t('systemManage.userId'), dataIndex: 'id', key: 'id', width: 100 },
    { title: t('systemManage.username'), dataIndex: 'username', key: 'username', width: 120 },
    { title: t('systemManage.role'), dataIndex: 'role', key: 'role', width: 120, render: getRoleBadge },
    { title: t('systemManage.email'), dataIndex: 'email', key: 'email', width: 200 },
    { title: t('systemManage.status'), dataIndex: 'status', key: 'status', width: 100, render: getStatusTag },
    { title: t('systemManage.createdAt'), dataIndex: 'createdAt', key: 'createdAt', width: 180 },
    {
      title: t('systemManage.actions'),
      key: 'actions',
      width: 360,
      render: (_, record) => (
        <div className="btn-group" style={{ display: 'flex', gap: '6px', flexWrap: 'wrap' }}>
          <button
            type="button"
            className="btn btn-sm"
            style={{ background: '#e8f4fd', color: 'var(--primary)', padding: '4px 10px', fontSize: '13px' }}
            onClick={() => handleAction('edit', record)}
          >
            {t('systemManage.edit')}
          </button>
          <button
            type="button"
            className="btn btn-sm"
            style={{ background: '#e6f7ed', color: 'var(--success)', padding: '4px 10px', fontSize: '13px' }}
            onClick={() => handleAction('change-password', record)}
          >
            修改密码
          </button>
          <button
            type="button"
            className="btn btn-sm"
            style={{ background: '#fff4e6', color: 'var(--warning)', padding: '4px 10px', fontSize: '13px' }}
            onClick={() => handleAction('reset-password', record)}
          >
            {t('systemManage.resetPassword')}
          </button>
          {record.status === 'active' ? (
            <button
              type="button"
              className="btn btn-sm"
              style={{ background: '#fef3f2', color: 'var(--danger)', padding: '4px 10px', fontSize: '13px' }}
              onClick={() => handleAction('disable', record)}
            >
              {t('systemManage.disable')}
            </button>
          ) : (
            <button
              type="button"
              className="btn btn-sm"
              style={{ background: '#e6f7ed', color: 'var(--success)', padding: '4px 10px', fontSize: '13px' }}
              onClick={() => handleAction('enable', record)}
            >
              {t('systemManage.enable')}
            </button>
          )}
          <button
            type="button"
            className="btn btn-sm"
            style={{ background: '#fef3f2', color: 'var(--danger)', padding: '4px 10px', fontSize: '13px' }}
            onClick={() => handleAction('delete', record)}
          >
            {t('common.delete')}
          </button>
        </div>
      ),
    },
  ];

  const menuItems = [
    { key: 'connections', label: t('connectionManage.title') },
    { key: 'users', label: '用户管理' },
    { key: 'logs', label: t('systemManage.operationLog') },
    { key: 'help', label: t('systemManage.helpCenter') },
  ];

  // 获取页面标题（与旧版本一致）
  const getPageTitle = () => {
    const titles = {
      connections: t('connectionManage.title'),
      users: t('systemManage.userList'),
      logs: t('systemManage.operationLog'),
      help: t('systemManage.helpCenter'),
    };
    return titles[activeKey] || t('systemManage.title');
  };

  return (
    <>
      <aside className="side-nav" data-group="system-manage">
        <h2>{t('systemManage.title')}</h2>
        <button
          type="button"
          className={`side-item ${activeKey === 'connections' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('connections')}
        >
          {t('connectionManage.title')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'users' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('users')}
        >
          {t('systemManage.userList')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'logs' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('logs')}
        >
          {t('systemManage.operationLog')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'help' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('help')}
        >
          {t('systemManage.helpCenter')}
        </button>
      </aside>

      <section className="page-panels">
        <article className="panel is-visible">
          {activeKey !== 'help' && (
            <header className="panel-header">
              <h2>{getPageTitle()}</h2>
              {activeKey === 'users' && (
                <button type="button" className="btn primary" onClick={handleAddUser}>
                  新增用户
                </button>
              )}
            </header>
          )}
          <section className="panel-body">
            {activeKey === 'connections' && <ConnectionManage />}
            {activeKey === 'users' && (
              <>
                <form className="form-grid">
                  <div className="form-row">
                    <label>{t('systemManage.username')}</label>
                    <input
                      type="text"
                      placeholder={t('systemManage.usernamePlaceholder')}
                      value={searchForm.username}
                      onChange={(e) => setSearchForm({ ...searchForm, username: e.target.value })}
                    />
                  </div>
                  <div className="form-row">
                    <label>{t('systemManage.role')}</label>
                    <select
                      value={searchForm.role}
                      onChange={(e) => setSearchForm({ ...searchForm, role: e.target.value })}
                    >
                      <option value="">{t('systemManage.all')}</option>
                      <option value="admin">{t('systemManage.admin')}</option>
                      <option value="operator">{t('systemManage.operator')}</option>
                      <option value="viewer">{t('systemManage.viewer')}</option>
                    </select>
                  </div>
                  <div className="form-row">
                    <label>{t('systemManage.status')}</label>
                    <select
                      value={searchForm.status}
                      onChange={(e) => setSearchForm({ ...searchForm, status: e.target.value })}
                    >
                      <option value="">{t('systemManage.all')}</option>
                      <option value="active">{t('systemManage.enable')}</option>
                      <option value="inactive">{t('systemManage.disable')}</option>
                    </select>
                  </div>
                  <div className="form-row-actions">
                    <button type="button" className="btn primary" onClick={handleSearch}>
                      {t('systemManage.query')}
                    </button>
                    <button type="button" className="btn ghost" onClick={handleReset}>
                      {t('systemManage.reset')}
                    </button>
                  </div>
                </form>

                <Table
                  rowKey="id"
                  loading={loading}
                  columns={columns}
                  dataSource={dataSource}
                  pagination={false}
                  className="data-table"
                />

                <div className="pagination">
                  <button type="button" className="btn ghost" disabled>
                    {t('systemManage.prevPage')}
                  </button>
                  <span className="pagination-info">{t('systemManage.pageInfo', { current: 1, total: 1 })}</span>
                  <button type="button" className="btn ghost" disabled>
                    {t('systemManage.nextPage')}
                  </button>
                </div>
              </>
            )}
            {activeKey === 'logs' && (
              <>
                <form className="form-grid">
                  <div className="form-row">
                    <label>{t('systemManage.operationPerson')}</label>
                    <input
                      type="text"
                      placeholder={t('systemManage.operationPersonPlaceholder')}
                      value={logSearchForm.operator}
                      onChange={(e) => setLogSearchForm({ ...logSearchForm, operator: e.target.value })}
                    />
                  </div>
                  <div className="form-row">
                    <label>{t('systemManage.operationType')}</label>
                    <select
                      value={logSearchForm.operation}
                      onChange={(e) => setLogSearchForm({ ...logSearchForm, operation: e.target.value })}
                    >
                      <option value="">{t('systemManage.all')}</option>
                      <option value="create">{t('systemManage.operationCreate')}</option>
                      <option value="update">{t('systemManage.operationUpdate')}</option>
                      <option value="delete">{t('common.delete')}</option>
                      <option value="publish">{t('systemRelease.publish')}</option>
                    </select>
                  </div>
                  <div className="form-row" style={{ gridColumn: 'span 2' }}>
                    <label>{t('systemManage.timeRange')}</label>
                    <div style={{ display: 'flex', gap: '10px', alignItems: 'center' }}>
                      <input
                        type="date"
                        value={logSearchForm.startDate}
                        onChange={(e) => setLogSearchForm({ ...logSearchForm, startDate: e.target.value })}
                        style={{ flex: 1 }}
                      />
                      <span>-</span>
                      <input
                        type="date"
                        value={logSearchForm.endDate}
                        onChange={(e) => setLogSearchForm({ ...logSearchForm, endDate: e.target.value })}
                        style={{ flex: 1 }}
                      />
                    </div>
                  </div>
                  <div className="form-row-actions">
                    <button
                      type="button"
                      className="btn primary"
                      onClick={loadLogs}
                    >
                      {t('systemManage.query')}
                    </button>
                    <button
                      type="button"
                      className="btn ghost"
                      onClick={() => {
                        setLogSearchForm({ operator: '', operation: '', startDate: '', endDate: '' });
                      }}
                    >
                      {t('systemManage.reset')}
                    </button>
                  </div>
                </form>

                <Table
                  rowKey="id"
                  loading={logLoading}
                  columns={[
                    { title: t('systemManage.operationId'), dataIndex: 'id', key: 'id', width: 80 },
                    { title: t('systemManage.operationPerson'), dataIndex: 'operator', key: 'operator', width: 110 },
                    {
                      title: t('systemManage.operationType'),
                      dataIndex: 'operation',
                      key: 'operation',
                      width: 100,
                      render: (operation) => {
                        const operationMap = {
                          publish: { label: t('systemRelease.publish'), className: 'badge--success' },
                          update: { label: t('systemManage.operationUpdate'), className: 'badge--info' },
                          delete: { label: t('systemManage.operationDelete'), className: 'badge--warning' },
                          create: { label: t('systemManage.operationCreate'), className: 'badge--primary' },
                          validate: { label: t('systemManage.operationValidate'), className: 'badge--secondary' },
                          login: { label: t('systemManage.operationLogin'), className: 'badge--secondary' },
                        };
                        const config = operationMap[operation] || { label: operation, className: 'badge--secondary' };
                        return <span className={`badge ${config.className}`}>{config.label}</span>;
                      },
                    },
                    { title: t('systemManage.operationTarget'), dataIndex: 'target', key: 'target', width: 220 },
                    { title: t('systemManage.operationDesc'), dataIndex: 'description', key: 'description', width: 160, ellipsis: true },
                    {
                      title: t('systemManage.operationContent'),
                      dataIndex: 'content',
                      key: 'content',
                      ellipsis: true,
                    },
                    {
                      title: t('systemManage.operationTime'),
                      dataIndex: 'updatedAt',
                      key: 'updatedAt',
                      width: 180,
                      render: (updatedAt) => {
                        if (!updatedAt) return '-';
                        // 将 UTC 时间转换为本地时间并格式化为标准格式
                        const date = new Date(updatedAt);
                        return date.toLocaleString('zh-CN', {
                          year: 'numeric',
                          month: '2-digit',
                          day: '2-digit',
                          hour: '2-digit',
                          minute: '2-digit',
                          second: '2-digit',
                          hour12: false,
                        });
                      },
                    },
                    {
                      title: t('systemManage.status'),
                      dataIndex: 'status',
                      key: 'status',
                      width: 100,
                      render: (status) => {
                        const statusMap = {
                          success: { label: t('systemManage.statusSuccess'), className: 'status-tag--success' },
                          error: { label: t('systemManage.statusError'), className: 'status-tag--error' },
                        };
                        const config = statusMap[status] || { label: status, className: 'status-tag--inactive' };
                        return <span className={`status-tag ${config.className}`}>{config.label}</span>;
                      },
                    },
                  ]}
                  dataSource={logDataSource}
                  pagination={false}
                  className="data-table"
                  scroll={{ x: 1200 }}
                />

                <div className="pagination">
                  <button type="button" className="btn ghost" disabled>
                    {t('systemManage.prevPage')}
                  </button>
                  <span className="pagination-info">{t('systemManage.pageInfo', { current: 1, total: 3 })}</span>
                  <button type="button" className="btn ghost">
                    {t('systemManage.nextPage')}
                  </button>
                </div>
              </>
            )}
            {activeKey === 'help' && (
              <>
                <div
                  style={{
                    display: 'flex',
                    gap: '16px',
                    height: 'calc(100vh - 200px)',
                  }}
                >
                  <iframe
                    src="https://docs.warpparse.ai/zh/10-user/01-cli/00-concepts-guide.html"
                    style={{
                      width: '100%',
                      height: '100%',
                      border: '1px solid #e5e7eb',
                      borderRadius: '12px',
                      background: '#ffffff',
                    }}
                    title="WarpParse 文档"
                  />
                </div>
              </>
            )}
          </section>
        </article>
      </section>

      {/* 新增/编辑用户弹窗 */}
      <Modal
        title={userModalMode === 'create' ? '新增用户' : '编辑用户'}
        open={userModalVisible}
        onOk={handleUserSubmit}
        onCancel={() => setUserModalVisible(false)}
        okText="确定"
        cancelText="取消"
      >
        <Form form={userForm} layout="vertical">
          <Form.Item
            label="用户名"
            name="username"
            rules={[{ required: true, message: '请输入用户名' }]}
          >
            <Input placeholder="请输入用户名" disabled={userModalMode === 'edit'} />
          </Form.Item>
          {userModalMode === 'create' && (
            <Form.Item
              label="密码"
              name="password"
              rules={[{ required: true, message: '请输入密码' }]}
            >
              <Input.Password placeholder="请输入密码" />
            </Form.Item>
          )}
          <Form.Item label="显示名称" name="displayName">
            <Input placeholder="请输入显示名称" />
          </Form.Item>
          <Form.Item label="邮箱" name="email">
            <Input placeholder="请输入邮箱" />
          </Form.Item>
          <Form.Item
            label="角色"
            name="role"
            rules={[{ required: true, message: '请选择角色' }]}
          >
            <Select placeholder="请选择角色">
              <Select.Option value="admin">管理员</Select.Option>
              <Select.Option value="operator">操作员</Select.Option>
              <Select.Option value="viewer">查看者</Select.Option>
            </Select>
          </Form.Item>
          <Form.Item label="备注" name="remark">
            <Input.TextArea placeholder="请输入备注" rows={3} />
          </Form.Item>
        </Form>
      </Modal>

      {/* 修改密码弹窗 */}
      <Modal
        title="修改密码"
        open={passwordModalVisible}
        onOk={handlePasswordSubmit}
        onCancel={() => setPasswordModalVisible(false)}
        okText="确定"
        cancelText="取消"
      >
        <Form form={passwordForm} layout="vertical">
          <Form.Item
            label="旧密码"
            name="oldPassword"
            rules={[{ required: true, message: '请输入旧密码' }]}
          >
            <Input.Password placeholder="请输入旧密码" />
          </Form.Item>
          <Form.Item
            label="新密码"
            name="newPassword"
            rules={[{ required: true, message: '请输入新密码' }]}
          >
            <Input.Password placeholder="请输入新密码" />
          </Form.Item>
          <Form.Item
            label="确认新密码"
            name="confirmPassword"
            rules={[
              { required: true, message: '请再次输入新密码' },
              ({ getFieldValue }) => ({
                validator(_, value) {
                  if (!value || getFieldValue('newPassword') === value) {
                    return Promise.resolve();
                  }
                  return Promise.reject(new Error('两次输入的密码不一致'));
                },
              }),
            ]}
          >
            <Input.Password placeholder="请再次输入新密码" />
          </Form.Item>
        </Form>
      </Modal>
    </>
  );
}

export default SystemManagePage;
