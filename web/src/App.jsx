import React, { useState, useEffect } from 'react';
import 'dayjs/locale/zh-cn';
import { Navigate, Route, Routes } from 'react-router-dom';
import { ConfigProvider, App as AntdApp } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import enUS from 'antd/locale/en_US';
import dayjs from 'dayjs';
import Navigation from '@/views/components/Navigation';
import RequireAuth from '@/views/components/RequireAuth';
import LoginPage from '@/views/pages/login';
import FeaturesPage from '@/views/pages/features';
import SystemReleasePage from '@/views/pages/system-release';
import ReleaseDetailPage from '@/views/pages/system-release/detail';
import PrepublishPage from '@/views/pages/system-release/prepublish';
import RuleManagePage from '@/views/pages/rule-manage';
import ConfigManagePage from '@/views/pages/config-manage';
import SimulateDebugPage from '@/views/pages/simulate-debug';
import SystemManagePage from '@/views/pages/system-manage';
import { AssistTaskProvider } from '@/contexts/AssistTaskContext';
import AssistTaskCenter from '@/views/components/AssistTaskCenter';

// 设置 dayjs 为中文语言环境
dayjs.locale('zh-cn');

// Ant Design 自定义主题配置
// 包括颜色系统、圆角、字体等，与原型设计保持一致
const theme = {
  token: {
    colorPrimary: '#275efe',
    colorSuccess: '#17b26a',
    colorWarning: '#f79009',
    colorError: '#f1554c',
    colorInfo: '#12a6e8',
    colorTextBase: '#1b2533',
    colorBgBase: '#ffffff',
    borderRadius: 12,
    fontFamily: '"PingFang SC", "Microsoft YaHei", "Segoe UI", sans-serif',
  },
  components: {
    Button: {
      borderRadius: 12,
      controlHeight: 40,
      fontSize: 14,
      fontWeight: 500,
    },
    Input: {
      borderRadius: 10,
      controlHeight: 40,
    },
    Card: {
      borderRadiusLG: 22,
      boxShadow: '0 18px 48px rgba(33, 47, 75, 0.12)',
    },
    Table: {
      borderRadius: 16,
      headerBg: 'rgba(27, 37, 51, 0.04)',
    },
    Menu: {
      itemBorderRadius: 12,
    },
  },
};

function App() {
  // 初始化时根据保存的语言设置 Ant Design locale
  const [antdLocale, setAntdLocale] = useState(zhCN);

  useEffect(() => {
    const savedLang = localStorage.getItem('language') || 'zh-CN';
    setAntdLocale(savedLang === 'zh-CN' ? zhCN : enUS);
  }, []);

  const handleLocaleChange = (newLocale) => {
    setAntdLocale(newLocale);
  };

  return (
    <ConfigProvider locale={antdLocale} theme={theme}>
      <AntdApp>
        <Routes>
          {/* 登录页面不包裹在 Navigation 中 */}
          <Route path="/login" element={<LoginPage />} />

          {/* 其他页面包裹在 Navigation 中 */}
          <Route
            path="/*"
            element={
              // AssistTaskProvider 在路由层内部，可安全使用 useNavigate
              <AssistTaskProvider>
                <Navigation onLocaleChange={handleLocaleChange}>
                  <RequireAuth>
                    <Routes>
                      <Route path="/" element={<Navigate to="/features" replace />} />
                      <Route path="/features" element={<FeaturesPage />} />
                      <Route path="/system-release" element={<SystemReleasePage />} />
                      <Route path="/system-release/:id" element={<ReleaseDetailPage />} />
                      <Route
                        path="/system-release/:id/prepublish"
                        element={<PrepublishPage />}
                      />
                      <Route path="/rule-manage" element={<RuleManagePage />} />
                      <Route path="/config-manage" element={<ConfigManagePage />} />
                      <Route path="/simulate-debug" element={<SimulateDebugPage />} />
                      <Route path="/system-manage" element={<SystemManagePage />} />
                      <Route path="*" element={<Navigate to="/features" replace />} />
                    </Routes>
                    {/* 全局任务中心悬浮按钮，在所有认证页面可见 */}
                    <AssistTaskCenter />
                  </RequireAuth>
                </Navigation>
              </AssistTaskProvider>
            }
          />
        </Routes>
      </AntdApp>
    </ConfigProvider>
  );
}

export default App;
