/**
 * 人工提单弹窗组件
 * 用户填写补充说明，选择需要分析的规则类型，提交人工辅助工单
 * 提交后由人工支持平台通过 reply 接口写回结果
 */

import { Checkbox, Form, Input, Modal, Space, Typography } from 'antd';
import React, { useEffect } from 'react';
import { useTranslation } from 'react-i18next';

const { TextArea } = Input;
const { Text } = Typography;

/**
 * 人工提单弹窗
 *
 * @param {Object} props
 * @param {boolean} props.open - 是否展开
 * @param {string} props.logData - 当前日志数据（自动填入，可编辑）
 * @param {string} [props.currentWpl] - 当前 WPL 规则（供参考）
 * @param {string} [props.currentOml] - 当前 OML 规则（供参考）
 * @param {'wpl'|'oml'|'both'} [props.defaultTargetRule] - 默认勾选的规则类型
 * @param {function} props.onSubmit - 提交回调，参数 { logData, extraNote, targetRule, currentRule }
 * @param {function} props.onClose - 取消/关闭回调
 */
function ManualTicketModal({
  open,
  logData,
  currentWpl,
  currentOml,
  defaultTargetRule = 'wpl',
  onSubmit,
  onClose,
}) {
  const { t } = useTranslation();
  const [form] = Form.useForm();

  // 弹窗打开时初始化表单值
  useEffect(() => {
    if (open) {
      const defaultChecked = defaultTargetRule === 'both'
        ? ['wpl', 'oml']
        : [defaultTargetRule];

      form.setFieldsValue({
        log_data: logData || '',
        extra_note: '',
        target_rules: defaultChecked,
      });
    }
  }, [open, logData, defaultTargetRule, form]);

  const handleSubmit = () => {
    form.validateFields().then((values) => {
      const { log_data: editedLogData, extra_note: extraNote, target_rules: targetRules } = values;

      // 将 checkbox 数组转换为 targetRule 字符串
      let targetRule = 'wpl';
      if (targetRules.includes('wpl') && targetRules.includes('oml')) {
        targetRule = 'both';
      } else if (targetRules.includes('oml')) {
        targetRule = 'oml';
      } else {
        targetRule = 'wpl';
      }

      // 合并当前规则作为参考（根据 targetRule 选择对应规则）
      let currentRule = '';
      if (targetRule === 'wpl' || targetRule === 'both') {
        currentRule = currentWpl || '';
      }
      if (targetRule === 'oml' || targetRule === 'both') {
        currentRule = currentRule
          ? `${currentRule}\n\n--- OML ---\n${currentOml || ''}`
          : (currentOml || '');
      }

      onSubmit({
        logData: editedLogData,
        extraNote: extraNote || '',
        targetRule,
        currentRule: currentRule || undefined,
      });
    });
  };

  return (
    <Modal
      title={t('assistTask.manualTicket')}
      open={open}
      onCancel={onClose}
      onOk={handleSubmit}
      okText={t('assistTask.submitTicket')}
      cancelText={t('assistTask.cancel')}
      width={600}
      destroyOnClose
    >
      <Form form={form} layout="vertical" style={{ marginTop: 8 }}>
        {/* 日志数据（可编辑） */}
        <Form.Item
          name="log_data"
          label={t('assistTask.logData')}
          rules={[{ required: true, message: t('assistTask.logDataRequired') }]}
        >
          <TextArea
            rows={5}
            placeholder={t('assistTask.logDataPlaceholder')}
            style={{
              fontFamily: '"JetBrains Mono", "Fira Code", monospace',
              fontSize: 12,
            }}
          />
        </Form.Item>

        {/* 补充说明 */}
        <Form.Item
          name="extra_note"
          label={
            <Space>
              {t('assistTask.extraNote')}
              <Text type="secondary" style={{ fontSize: 12 }}>
                {t('assistTask.optional')}
              </Text>
            </Space>
          }
        >
          <TextArea
            rows={3}
            placeholder={t('assistTask.extraNotePlaceholder')}
          />
        </Form.Item>

        {/* 目标规则类型 */}
        <Form.Item
          name="target_rules"
          label={t('assistTask.targetRuleType')}
          rules={[
            {
              validator: (_, value) =>
                value && value.length > 0
                  ? Promise.resolve()
                  : Promise.reject(new Error(t('assistTask.targetRuleRequired'))),
            },
          ]}
        >
          <Checkbox.Group>
            <Checkbox value="wpl">{t('assistTask.ruleType.wpl')}</Checkbox>
            <Checkbox value="oml">{t('assistTask.ruleType.oml')}</Checkbox>
          </Checkbox.Group>
        </Form.Item>
      </Form>
    </Modal>
  );
}

export default ManualTicketModal;
