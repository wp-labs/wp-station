import React from 'react';

/**
 * 数据采集页面
 * 功能：展示数据采集能力示意图
 * 对应原型：pages/views/data-collect.html
 */
function FeaturesPage() {
  return (
    <section className="page-panels">
      <article className="panel is-visible">
        <div
          style={{
            width: '100%',
            height: '100%',
            borderRadius: '16px',
            overflow: 'hidden',
          }}
        >
          <iframe
            src="http://localhost:18080/wp-monitor"
            title="数据采集监控"
            style={{
              width: '100%',
              height: '100%',
              border: 'none',
            }}
          />
        </div>
      </article>
    </section>
  );
}

export default FeaturesPage;
