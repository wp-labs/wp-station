import React, { useState, useEffect, useRef } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Button } from 'antd';
import { RedditOutlined, SlackOutlined, GithubOutlined, WechatOutlined, QuestionCircleOutlined } from '@ant-design/icons';
import { getSessionUser, logout } from '@/services/auth';
import httpRequest from '@/services/request';
import LanguageSwitcher from '@/views/components/LanguageSwitcher';
import GitHubStarModal from '@/views/components/GitHubStarModal';
import WechatModal from '@/views/components/WechatModal';

/**
 * 顶部导航组件
 * 功能：
 * 1. 显示品牌 Logo 和导航菜单
 * 2. 显示连接状态和用户信息
 * 3. 提供用户登出功能
 * 4. 登录页不显示导航
 * 对应原型：pages/views/*.html 中的 main-header
 */
function Navigation({ children, onLocaleChange }) {
  const navigate = useNavigate();
  const location = useLocation();
  const { t } = useTranslation();
  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const userMenuRef = useRef(null);
  const [versionInfo, setVersionInfo] = useState({ warpStation: '', warpParse: '' });
  const [wechatModalOpen, setWechatModalOpen] = useState(false);
  const [githubModalOpen, setGithubModalOpen] = useState(false);

  // 获取当前登录用户名
  const sessionUser = getSessionUser();
  const usernameLabel = sessionUser?.username || '';

  // 获取版本信息：wp-station 与 warp-parse
  useEffect(() => {
    const fetchVersion = async () => {
      try {
        const response = await httpRequest.get('/version');
        setVersionInfo({
          warpStation: response?.wp_station || '',
          warpParse: response?.warp_parse || '',
        });
      } catch (_error) {
        // 忽略版本获取失败，不影响主流程
      }
    };

    fetchVersion();
  }, []);

  const menuItems = [
    { path: '/features', name: t('navigation.dataCollection'), page: 'data-collect' },
    { path: '/system-release', name: t('navigation.systemRelease'), page: 'system-release' },
    { path: '/rule-manage', name: t('navigation.ruleConfig'), page: 'rule-manage' },
    { path: '/config-manage', name: t('navigation.configManage'), page: 'config-manage' },
    { path: '/simulate-debug', name: t('navigation.simulateDebug'), page: 'simulate-debug' },
    { path: '/system-manage', name: t('navigation.systemManage'), page: 'system-manage' },
  ];

  /**
   * 判断导航菜单项是否激活
   * @param {string} path - 菜单项路径
   * @returns {boolean} 是否激活
   */
  const isActive = (path) => {
    return location.pathname === path || location.pathname.startsWith(`${path}/`);
  };

  /**
   * 处理用户登出
   * 清除会话信息并跳转到登录页
   */
  const handleLogout = () => {
    logout();
    navigate('/login', { replace: true });
  };

  /**
   * 处理用户菜单切换
   */
  const handleUserMenuToggle = (e) => {
    e.stopPropagation();
    setUserMenuOpen(!userMenuOpen);
  };

  // 点击外部关闭用户菜单
  useEffect(() => {
    const handleClickOutside = (event) => {
      if (userMenuRef.current && !userMenuRef.current.contains(event.target)) {
        setUserMenuOpen(false);
      }
    };

    if (userMenuOpen) {
      document.addEventListener('click', handleClickOutside);
    }

    return () => {
      document.removeEventListener('click', handleClickOutside);
    };
  }, [userMenuOpen]);

  // 登录页不显示导航
  if (location.pathname === '/login') {
    return <>{children}</>;
  }

  return (
    // 应用整体布局：头部固定在上方，下面内容区域单独滚动
    <div className="app-shell">
      <header className="main-header">
        <div className="brand">
          <img src="/assets/images/index.png" alt="WarpStation" className="logo" style={{ height: '70px' }} />
          <span className="divider">|</span>
          <span className="subtitle">{t('navigation.controlPlatform')}</span>
          {versionInfo.warpStation || versionInfo.warpParse ? (
            <span
              className="version-info"
              style={{ marginLeft: 8, fontSize: 12, color: '#fff' }}
            >
              {versionInfo.warpStation && (
                <span style={{ marginRight: 8 }}>wp-station: {versionInfo.warpStation}</span>
              )}<br/>
              {versionInfo.warpParse && <span>warp-parse: {versionInfo.warpParse}</span>}
            </span>
          ) : null}
        </div>
        <nav className="top-nav">
          {menuItems.map((menuItem) => (
            <button
              key={menuItem.path}
              type="button"
              className={`nav-item ${isActive(menuItem.path) ? 'is-active' : ''}`}
              data-page={menuItem.page}
              onClick={() => navigate(menuItem.path)}
            >
              {menuItem.name}
            </button>
          ))}
        </nav>
        <div className="header-actions">
          <Button
            type="primary"
            icon={<SlackOutlined style={{ fontSize: '18px' }} />}
            size="large"
            style={{ fontWeight: 600, fontSize: '15px' }}
            onClick={() => window.open('https://app.slack.com/client/T0A53FLT4R4/C0A4Q3SC2CF', '_blank')}
          >
            {t('header.slack')}
          </Button>
          <Button
            type="primary"
            icon={<RedditOutlined style={{ fontSize: '18px' }} />}
            size="large"
            style={{ fontWeight: 600, fontSize: '15px' }}
            onClick={() => window.open('https://www.reddit.com/r/warppase/', '_blank')}
          >
            {t('header.reddit')}
          </Button>
          <Button
            type="primary"
            icon={<GithubOutlined style={{ fontSize: '18px' }} />}
            size="large"
            style={{ fontWeight: 600, fontSize: '15px' }}
            onClick={() => setGithubModalOpen(true)}
          >
            {t('header.github')}
          </Button>
          <Button
            type="primary"
            icon={
              <svg
                viewBox="0 0 36 28"
                xmlns="http://www.w3.org/2000/svg"
                style={{ width: '18px', height: '18px', fill: 'currentColor' }}
              >
                <path d="M17.5875 6.77268L21.8232 3.40505L17.5875 0.00748237L17.5837 0L13.3555 3.39757L17.5837 6.76894L17.5875 6.77268ZM17.5863 17.3955H17.59L28.5161 8.77432L25.5526 6.39453L17.59 12.6808H17.5863L17.5825 12.6845L9.61993 6.40201L6.66016 8.78181L17.5825 17.3992L17.5863 17.3955ZM17.5828 23.2891L17.5865 23.2854L32.2133 11.7456L35.1768 14.1254L28.5238 19.3752L17.5865 28L0.284376 14.3574L0 14.1291L2.95977 11.7531L17.5828 23.2891Z" />
              </svg>
            }
            size="large"
            style={{ fontWeight: 600, fontSize: '15px' }}
            onClick={() => window.open('https://juejin.cn/user/239030525498106', '_blank')}
          >
            {t('header.juejin')}
          </Button>
          <Button
            type="primary"
            icon={<QuestionCircleOutlined style={{ fontSize: '18px' }} />}
            size="large"
            style={{ fontWeight: 600, fontSize: '15px' }}
            onClick={() => window.open('https://wp-labs.github.io/wp-docs/', '_blank')}
          >
            {t('header.helpCenter')}
          </Button>
          <Button
            type="primary"
            icon={<WechatOutlined style={{ fontSize: '20px' }} />}
            size="large"
            shape="circle"
            style={{ background: '#07C160', borderColor: '#07C160' }}
            onClick={() => setWechatModalOpen(true)}
          />
          <LanguageSwitcher onLocaleChange={onLocaleChange} />
          <div className={`user-menu ${userMenuOpen ? 'active' : ''}`} id="user-menu" ref={userMenuRef}>
            <button
              type="button"
              className="user-trigger"
              id="user-trigger"
              onClick={handleUserMenuToggle}
            >
              <span className="user-icon">👤</span>
              <span className="user-name" id="user-name">
                {usernameLabel}
              </span>
            </button>
            <div className="user-dropdown">
              <button
                type="button"
                className="user-dropdown-item"
                id="logout-btn"
                onClick={handleLogout}
              >
                {t('navigation.logout')}
              </button>
            </div>
          </div>
        </div>
      </header>
      <div className="app-shell-body">
        <div
          className={
            ['/features', '/system-release'].some((path) =>
              location.pathname === path || location.pathname.startsWith(`${path}/`)
            )
              ? 'main-content no-side-nav'
              : 'main-content'
          }
        >
          {children}
        </div>
      </div>
      
      {/* 微信群二维码弹窗 */}
      <WechatModal open={wechatModalOpen} onCancel={() => setWechatModalOpen(false)} />
      
      {/* GitHub Star 弹窗 */}
      <GitHubStarModal
        open={githubModalOpen}
        onCancel={() => setGithubModalOpen(false)}
        onGoToGitHub={() => {
          window.open('https://github.com/wp-labs/warp-parse', '_blank');
          setGithubModalOpen(false);
        }}
      />
    </div>
  );
}

export default Navigation;
