import React, { useState, useEffect } from 'react';
import { Form, Input, message } from 'antd';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { login } from '@/services/auth';

const CAPTCHA_CHARACTERS = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

function generateCaptcha(length = 4) {
  return Array.from(
    { length },
    () => CAPTCHA_CHARACTERS[Math.floor(Math.random() * CAPTCHA_CHARACTERS.length)],
  ).join('');
}

/**
 * 登录页面
 * 功能：
 * 1. 提供必填的用户名、密码和可留空的本地验证码
 * 2. 调用登录 API 进行身份验证
 * 3. 登录成功后跳转到规则管理主页
 * 对应原型：pages/views/login.html
 */
function LoginPage() {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [form] = Form.useForm();
  const [loading, setLoading] = useState(false);
  const [captchaCode, setCaptchaCode] = useState(() => generateCaptcha());

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
   * @param {string} formValues.captcha - 本地验证码，允许留空
   */
  const handleFinish = async (formValues) => {
    const captchaInput = formValues.captcha?.trim();
    if (captchaInput && captchaInput.toUpperCase() !== captchaCode) {
      form.setFields([
        {
          name: 'captcha',
          value: '',
          errors: [t('login.captchaInvalid')],
        },
      ]);
      setCaptchaCode(generateCaptcha());
      return;
    }

    setLoading(true);
    try {
      // 验证码只在浏览器本地校验，不传递给后端。
      const result = await login({
        username: formValues.username?.trim(),
        password: formValues.password,
      });

      // 登录成功，跳转到主页面
      message.success(`欢迎回来，${result.display_name || result.username}！`);
      navigate('/rule-manage', { replace: true });
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
          <label htmlFor="captcha">{t('login.captcha')}</label>
          <div className="captcha-group">
            <Form.Item name="captcha" className="captcha-input" style={{ marginBottom: 0 }}>
              <Input
                id="captcha"
                className="form-input"
                placeholder={t('login.captchaPlaceholder')}
                autoComplete="off"
                maxLength={4}
                onKeyPress={handleKeyPress}
                disabled={loading}
              />
            </Form.Item>
            <button
              type="button"
              className="captcha-display"
              title={t('login.captchaTitle')}
              aria-label={t('login.captchaTitle')}
              onClick={() => {
                setCaptchaCode(generateCaptcha());
                form.setFieldValue('captcha', '');
              }}
              disabled={loading}
            >
              {captchaCode}
            </button>
          </div>
        </div>

        <button type="submit" className="login-btn" disabled={loading}>
          {loading ? t('login.loggingIn') : t('login.loginButton')}
        </button>
      </Form>
    </div>
  );
}

export default LoginPage;
