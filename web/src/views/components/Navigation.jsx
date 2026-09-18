import React, { useState, useEffect } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Button } from 'antd';
import {
  RedditOutlined,
  SlackOutlined,
  GithubOutlined,
  WechatOutlined,
} from '@ant-design/icons';
import { useSystem } from '@/contexts/SystemContext';
import {
  fetchDataCollectConfig,
} from '@/services/features';
import httpRequest from '@/services/request';
import LanguageSwitcher from '@/views/components/LanguageSwitcher';
import GitHubStarModal from '@/views/components/GitHubStarModal';
import WechatModal from '@/views/components/WechatModal';

/**
 * 顶部导航组件
 * 功能：
 * 1. 显示品牌 Logo、系统切换和导航菜单
 * 2. 显示运行版本与外部社区入口
 * 3. 提供语言切换
 * 对应原型：pages/views/*.html 中的 main-header
 */
function Navigation({ children, onLocaleChange }) {
  const navigate = useNavigate();
  const location = useLocation();
  const { t } = useTranslation();
  const [versionInfo, setVersionInfo] = useState({
    warpStation: '',
    wparse: '',
    wfusion: '',
  });
  const [runtimeMonitorUrl, setRuntimeMonitorUrl] = useState('');
  const [wechatModalOpen, setWechatModalOpen] = useState(false);
  const [githubModalOpen, setGithubModalOpen] = useState(false);
  const { currentSystem, isSwitchingSystem, switchSystem, availableSystems, decoratePath } =
    useSystem();

  // 获取 Station 及两个运行系统的版本信息
  useEffect(() => {
    const fetchVersion = async () => {
      try {
        const response = await httpRequest.get('/version');
        setVersionInfo({
          warpStation: response?.wp_station || '',
          wparse: response?.wp_parse || '',
          wfusion: response?.wfusion || '',
        });
      } catch (_error) {
        // 忽略版本获取失败，不影响主流程
      }
    };

    fetchVersion();
  }, []);

  useEffect(() => {
    const fetchRuntimeMonitorUrl = async () => {
      try {
        const response = await fetchDataCollectConfig();
        if (response?.data_collect_url || response?.default_data_collect_url) {
          setRuntimeMonitorUrl(
            response.data_collect_url || response.default_data_collect_url || '',
          );
        }
      } catch (_error) {
        // 忽略运行监控地址获取失败
      }
    };

    fetchRuntimeMonitorUrl();
  }, []);

  const runtimeMonitorMenuItem = {
    path: '/features',
    name: t('navigation.dataCollection'),
    page: 'data-collect',
    external: true,
  };
  const simulateDebugMenuItem = {
    path: currentSystem === 'wfusion' ? '/wfusion-rule-editor' : '/simulate-debug',
    name: t('navigation.simulateDebug'),
    page: 'simulate-debug',
  };
  const primaryMenuItems = [
    {
      path: '/system-release',
      name: t('navigation.systemRelease'),
      page: 'system-release',
    },
    { path: '/rule-manage', name: t('navigation.ruleConfig'), page: 'rule-manage' },
    { path: '/config-manage', name: t('navigation.configManage'), page: 'config-manage' },
    simulateDebugMenuItem,
    {
      path: '/integration-overview',
      name: t('navigation.integrationOverview'),
      page: 'integration-overview',
    },
    { path: '/system-manage', name: t('connectionManage.title'), page: 'system-manage' },
  ];

  /**
   * 判断导航菜单项是否激活
   * @param {string} path - 菜单项路径
   * @returns {boolean} 是否激活
   */
  const isActive = (path) => {
    return location.pathname === path || location.pathname.startsWith(`${path}/`);
  };

  const handleMenuNavigate = (menuItem) => {
    if (menuItem.external) {
      if (runtimeMonitorUrl) {
        window.open(runtimeMonitorUrl, '_blank', 'noopener,noreferrer');
      }
      return;
    }

    navigate(decoratePath(menuItem.path));
  };

  const hasVersionInfo = Boolean(
    versionInfo.warpStation || versionInfo.wparse || versionInfo.wfusion,
  );

  return (
    // 应用整体布局：头部固定在上方，下面内容区域单独滚动
    <div className="app-shell">
      <header className="main-header">
        <div className={`system-switch-progress ${isSwitchingSystem ? 'is-visible' : ''}`} />
        <div className="header-top-row">
          <div className="brand">
            <img src="/assets/images/index.png" alt="WarpStation" className="logo" style={{ height: '70px' }} />
            <span className="divider">|</span>
            <span className="subtitle">{t('navigation.controlPlatform')}</span>
            <div className="header-system-context">
              <div
                className="system-switcher"
                role="tablist"
                aria-label={t('navigation.switchSystem')}
              >
                {availableSystems.map((systemKey) => {
                  const isActiveSystem = currentSystem === systemKey;
                  return (
                    <button
                      key={systemKey}
                      type="button"
                      role="tab"
                      aria-selected={isActiveSystem}
                      className={`system-switcher__item ${isActiveSystem ? 'is-active' : ''}`}
                      onClick={() => switchSystem(systemKey)}
                      disabled={isSwitchingSystem}
                    >
                      {t(`navigation.system.${systemKey}`)}
                    </button>
                  );
                })}
              </div>
            </div>
            {hasVersionInfo ? (
              <span
                className="version-info"
                style={{
                  marginLeft: 8,
                  fontSize: 12,
                  color: '#ffffff',
                  display: 'inline-flex',
                  flexDirection: 'column',
                  lineHeight: 1.3,
                }}
              >
                {versionInfo.warpStation && (
                  <span style={{ marginRight: 8 }}>wp-station: {versionInfo.warpStation}</span>
                )}
                {versionInfo.wparse && <span>wparse: {versionInfo.wparse}</span>}
                {versionInfo.wfusion && <span>wfusion: {versionInfo.wfusion}</span>}
              </span>
            ) : null}
          </div>
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
              icon={<WechatOutlined style={{ fontSize: '20px' }} />}
              size="large"
              shape="circle"
              style={{ background: '#07C160', borderColor: '#07C160' }}
              onClick={() => setWechatModalOpen(true)}
            />
            <LanguageSwitcher onLocaleChange={onLocaleChange} />
          </div>
        </div>
        <div className="header-nav-row">
          <nav className="top-nav">
            {primaryMenuItems.map((menuItem) => (
              <button
                key={menuItem.path}
                type="button"
                className={`nav-item ${isActive(menuItem.path) ? 'is-active' : ''}`}
                data-page={menuItem.page}
                onClick={() => handleMenuNavigate(menuItem)}
              >
                {menuItem.name}
              </button>
            ))}
          </nav>
          <div className="top-nav-monitor">
            <button
              type="button"
              className={`nav-item ${isActive(runtimeMonitorMenuItem.path) ? 'is-active' : ''}`}
              data-page={runtimeMonitorMenuItem.page}
              onClick={() => handleMenuNavigate(runtimeMonitorMenuItem)}
            >
              {runtimeMonitorMenuItem.name}
            </button>
          </div>
        </div>
      </header>
      <div className="app-shell-body">
        <div
          className={
            ['/system-release', '/integration-overview', '/system-manage'].some((path) =>
              location.pathname === path || location.pathname.startsWith(`${path}/`)
            )
              ? 'main-content no-side-nav'
              : 'main-content'
          }
        >
          <React.Fragment key={`${currentSystem}:${location.pathname}`}>
            {children}
          </React.Fragment>
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
