import React, { useState, useEffect } from 'react';
import { Form, Input, message } from 'antd';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { login } from '@/services/auth';

/**
 * 登录页面
 * 功能：
 * 1. 提供用户名、密码和验证码输入（均为可选）
 * 2. 调用登录 API 进行身份验证
 * 3. 登录成功后跳转到连接管理页
 * 对应原型：pages/views/login.html
 */
function LoginPage() {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [form] = Form.useForm();
  const [loading, setLoading] = useState(false);

  /**
   * 添加登录页面背景样式
   */
  useEffect(() => {
    document.body.classList.add('login-page');
    return () => {
      document.body.classList.remove('login-page');
    };
  }, []);

  /**
   * 处理表单提交
   * @param {Object} formValues - 表单值
   * @param {string} formValues.username - 用户名
   * @param {string} formValues.password - 密码
   * @param {string} formValues.captcha - 验证码（暂不校验）
   */
  const handleFinish = async (formValues) => {
    setLoading(true);
    try {
      // 调用登录 API（验证码暂不传递给后端）
      const result = await login({
        username: formValues.username?.trim(),
        password: formValues.password,
      });

      // 登录成功，跳转到主页面
      message.success(`欢迎回来，${result.display_name || result.username}！`);
      navigate('/features', { replace: true });
    } catch (error) {
      message.error(error.message || '登录失败，请检查用户名和密码');
    } finally {
      setLoading(false);
    }
  };

  /**
   * 处理回车键提交
   */
  const handleKeyPress = (e) => {
    if (e.key === 'Enter') {
      form.submit();
    }
  };

  return (
    <div className="login-container">
      <div className="login-header">
        <div className="login-logo">
          <img src="/assets/images/home.png" alt="WarpStation" style={{ height: '110px' }} />
        </div>
        <div className="login-subtitle">{t('login.title')}</div>
      </div>

      <Form form={form} onFinish={handleFinish} layout="vertical">
        <div className="form-group">
          <label htmlFor="username">{t('login.username')}</label>
          <Form.Item
            name="username"
            noStyle
            rules={[{ required: true, message: '请输入用户名' }]}
          >
            <Input
              id="username"
              className="form-input"
              placeholder={t('login.usernamePlaceholder')}
              autoComplete="username"
              onKeyPress={handleKeyPress}
              disabled={loading}
            />
          </Form.Item>
        </div>

        <div className="form-group">
          <label htmlFor="password">{t('login.password')}</label>
          <Form.Item
            name="password"
            noStyle
            rules={[{ required: true, message: '请输入密码' }]}
          >
            <Input.Password
              id="password"
              className="form-input"
              placeholder={t('login.passwordPlaceholder')}
              autoComplete="current-password"
              onKeyPress={handleKeyPress}
              disabled={loading}
            />
          </Form.Item>
        </div>

        <div className="form-group">
          <label htmlFor="captcha">验证码</label>
          <Form.Item
            name="captcha"
            noStyle
          >
            <Input
              id="captcha"
              className="form-input"
              placeholder="请输入验证码（可选）"
              autoComplete="off"
              onKeyPress={handleKeyPress}
              disabled={loading}
            />
          </Form.Item>
        </div>

        <button type="submit" className="login-btn" disabled={loading}>
          {loading ? t('login.loggingIn') : t('login.loginButton')}
        </button>
      </Form>
    </div>
  );
}

export default LoginPage;
