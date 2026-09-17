import React from 'react';
import { useTranslation } from 'react-i18next';
import ConnectionManage from './ConnectionManage';

/**
 * 连接管理页。
 *
 * 系统管理中的用户和帮助中心已移除，连接管理作为独立入口展示。
 */
function SystemManagePage() {
  const { t } = useTranslation();

  return (
    <section className="page-panels connection-manage-page">
      <article className="panel is-visible">
        <header className="panel-header">
          <h2>{t('connectionManage.title')}</h2>
        </header>
        <section className="panel-body">
          <ConnectionManage />
        </section>
      </article>
    </section>
  );
}

export default SystemManagePage;
