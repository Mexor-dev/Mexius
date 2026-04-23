import { useEffect, useMemo, useState } from 'react';
import { ArrowDownToLine, LoaderCircle, RotateCw, Zap } from 'lucide-react';
import SectionCard from '../controls/SectionCard';
import FieldRow from '../controls/FieldRow';
import NumberInput from '../controls/NumberInput';
import Slider from '../controls/Slider';
import Select from '../controls/Select';
import { getOllamaModels, pullOllamaModel } from '@/lib/api';
import { t } from '@/lib/i18n';

interface Props {
  config: Record<string, unknown>;
  onUpdate: (field: string, value: unknown) => void;
  onCommit: (updates: Array<{ path: string; value: unknown }>) => Promise<void>;
}

const LOCALE_OPTIONS = [
  { value: '', label: 'Auto-detect' },
  { value: 'en', label: 'English' },
  { value: 'zh', label: '中文' },
  { value: 'tr', label: 'Türkçe' },
];

const PROVIDER_OPTIONS = [
  { value: 'openrouter', label: 'OpenRouter' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'ollama', label: 'Ollama' },
  { value: 'gemini', label: 'Google Gemini' },
  { value: 'azure-openai', label: 'Azure OpenAI' },
  { value: 'bedrock', label: 'AWS Bedrock' },
  { value: 'groq', label: 'Groq' },
  { value: 'mistral', label: 'Mistral' },
  { value: 'deepseek', label: 'DeepSeek' },
  { value: 'xai', label: 'xAI (Grok)' },
  { value: 'together', label: 'Together AI' },
  { value: 'fireworks', label: 'Fireworks AI' },
  { value: 'perplexity', label: 'Perplexity' },
  { value: 'cohere', label: 'Cohere' },
  { value: 'cerebras', label: 'Cerebras' },
  { value: 'sambanova', label: 'SambaNova' },
  { value: 'lmstudio', label: 'LM Studio' },
  { value: 'llamacpp', label: 'llama.cpp' },
  { value: 'vllm', label: 'vLLM' },
  { value: 'qwen', label: 'Qwen' },
  { value: 'deepinfra', label: 'DeepInfra' },
  { value: 'huggingface', label: 'Hugging Face' },
  { value: 'nvidia', label: 'NVIDIA NIM' },
  { value: 'cloudflare', label: 'Cloudflare AI' },
  { value: 'litellm', label: 'LiteLLM' },
];

// Models grouped by provider. Newest models listed first.
const MODELS_BY_PROVIDER: Record<string, { value: string; label: string }[]> = {
  openrouter: [
    { value: 'anthropic/claude-sonnet-4-6', label: 'Claude Sonnet 4.6' },
    { value: 'anthropic/claude-opus-4-6', label: 'Claude Opus 4.6' },
    { value: 'anthropic/claude-4.5-sonnet', label: 'Claude 4.5 Sonnet' },
    { value: 'anthropic/claude-opus-4-20250514', label: 'Claude Opus 4' },
    { value: 'openai/gpt-5.4', label: 'GPT-5.4' },
    { value: 'openai/gpt-5.4-pro', label: 'GPT-5.4 Pro' },
    { value: 'openai/gpt-4o', label: 'GPT-4o' },
    { value: 'google/gemini-3.1-pro', label: 'Gemini 3.1 Pro' },
    { value: 'google/gemini-3.1-flash-lite', label: 'Gemini 3.1 Flash Lite' },
    { value: 'google/gemini-2.5-pro', label: 'Gemini 2.5 Pro' },
    { value: 'deepseek/deepseek-v3.2', label: 'DeepSeek V3.2' },
    { value: 'deepseek/deepseek-r1-0528', label: 'DeepSeek R1' },
    { value: 'x-ai/grok-4.1-fast', label: 'Grok 4.1 Fast' },
    { value: 'meta-llama/llama-4-maverick', label: 'Llama 4 Maverick 400B' },
    { value: 'meta-llama/llama-4-70b', label: 'Llama 4 70B' },
    { value: 'mistralai/devstral-2', label: 'Devstral 2' },
    { value: 'qwen/qwen-3.6-plus-preview', label: 'Qwen 3.6 Plus Preview' },
  ],
  anthropic: [
    { value: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6' },
    { value: 'claude-opus-4-6', label: 'Claude Opus 4.6' },
    { value: 'claude-4.5-sonnet', label: 'Claude 4.5 Sonnet' },
    { value: 'claude-opus-4-20250514', label: 'Claude Opus 4' },
    { value: 'claude-haiku-4-5-20251001', label: 'Claude Haiku 4.5' },
  ],
  openai: [
    { value: 'gpt-5.4', label: 'GPT-5.4' },
    { value: 'gpt-5.4-pro', label: 'GPT-5.4 Pro' },
    { value: 'gpt-4o', label: 'GPT-4o' },
    { value: 'gpt-4o-mini', label: 'GPT-4o Mini' },
    { value: 'o1-preview', label: 'o1 Preview' },
  ],
  gemini: [
    { value: 'gemini-3.1-pro', label: 'Gemini 3.1 Pro' },
    { value: 'gemini-3.1-flash-lite', label: 'Gemini 3.1 Flash Lite' },
    { value: 'gemini-3-pro', label: 'Gemini 3 Pro' },
    { value: 'gemini-2.5-pro', label: 'Gemini 2.5 Pro' },
    { value: 'gemini-2.5-flash', label: 'Gemini 2.5 Flash' },
  ],
  groq: [
    { value: 'llama-4-70b', label: 'Llama 4 70B' },
    { value: 'gpt-oss-120b', label: 'GPT-OSS 120B' },
    { value: 'llama-3.3-70b-versatile', label: 'Llama 3.3 70B' },
  ],
  mistral: [
    { value: 'mistral-large-latest', label: 'Mistral Large' },
    { value: 'devstral-2', label: 'Devstral 2' },
    { value: 'mistral-small-latest', label: 'Mistral Small' },
    { value: 'codestral-latest', label: 'Codestral' },
  ],
  deepseek: [
    { value: 'deepseek-chat', label: 'DeepSeek V3.2 Chat' },
    { value: 'deepseek-reasoner', label: 'DeepSeek R1 Reasoner' },
  ],
  xai: [
    { value: 'grok-4.1-fast', label: 'Grok 4.1 Fast' },
    { value: 'grok-3', label: 'Grok 3' },
    { value: 'grok-3-mini', label: 'Grok 3 Mini' },
  ],
  together: [
    { value: 'meta-llama/Llama-4-Maverick-400B', label: 'Llama 4 Maverick 400B' },
    { value: 'meta-llama/Llama-4-70B', label: 'Llama 4 70B' },
    { value: 'meta-llama/Llama-3.3-70B-Instruct-Turbo', label: 'Llama 3.3 70B Turbo' },
  ],
  fireworks: [
    { value: 'accounts/fireworks/models/llama-4-maverick-400b', label: 'Llama 4 Maverick 400B' },
    { value: 'accounts/fireworks/models/llama-v3p3-70b-instruct', label: 'Llama 3.3 70B' },
  ],
  cerebras: [
    { value: 'llama-4-70b', label: 'Llama 4 70B' },
    { value: 'llama-3.3-70b', label: 'Llama 3.3 70B' },
  ],
  bedrock: [
    { value: 'anthropic.claude-sonnet-4-6', label: 'Claude Sonnet 4.6' },
    { value: 'anthropic.claude-opus-4-6', label: 'Claude Opus 4.6' },
    { value: 'anthropic.claude-haiku-4-5', label: 'Claude Haiku 4.5' },
  ],
  'azure-openai': [
    { value: 'gpt-5.4', label: 'GPT-5.4' },
    { value: 'gpt-4o', label: 'GPT-4o' },
    { value: 'gpt-4o-mini', label: 'GPT-4o Mini' },
  ],
  qwen: [
    { value: 'qwen-3.6-plus-preview', label: 'Qwen 3.6 Plus Preview' },
    { value: 'qwen-max', label: 'Qwen Max' },
    { value: 'qwen-plus', label: 'Qwen Plus' },
    { value: 'qwen-turbo', label: 'Qwen Turbo' },
  ],
  perplexity: [
    { value: 'sonar-pro', label: 'Sonar Pro' },
    { value: 'sonar', label: 'Sonar' },
  ],
  sambanova: [
    { value: 'llama-4-maverick-400b', label: 'Llama 4 Maverick 400B' },
    { value: 'llama-3.3-70b', label: 'Llama 3.3 70B' },
  ],
};

export default function GeneralSection({ config, onUpdate, onCommit }: Props) {
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [ollamaReachable, setOllamaReachable] = useState<boolean | null>(null);
  const [loadingOllama, setLoadingOllama] = useState(false);
  const [ollamaError, setOllamaError] = useState<string | null>(null);
  const [pullModelName, setPullModelName] = useState('');
  const [pullingModel, setPullingModel] = useState(false);
  const [pullMessage, setPullMessage] = useState<string | null>(null);
  const provider = (config.default_provider as string) ?? 'openrouter';
  const currentModel = (config.default_model as string) ?? '';
  const modelOptions = useMemo(() => {
    if (provider === 'ollama') {
      return ollamaModels.map((model) => ({ value: model, label: model }));
    }
    return MODELS_BY_PROVIDER[provider];
  }, [ollamaModels, provider]);

  const loadOllamaModels = async () => {
    setLoadingOllama(true);
    setOllamaError(null);
    try {
      const data = await getOllamaModels();
      setOllamaReachable(data.reachable);
      setOllamaModels(data.models);
      if (data.models.length > 0 && (!currentModel || !data.models.includes(currentModel))) {
        onUpdate('default_model', data.models[0]);
      }
      if (!data.reachable && data.error) {
        setOllamaError(data.error);
      }
    } catch (error: unknown) {
      setOllamaReachable(false);
      setOllamaModels([]);
      setOllamaError(error instanceof Error ? error.message : 'Failed to load Ollama models');
    } finally {
      setLoadingOllama(false);
    }
  };

  useEffect(() => {
    if (provider === 'ollama') {
      loadOllamaModels();
    }
  }, [provider]);

  // When provider changes, auto-select the first model for that provider
  const handleProviderChange = (v: string) => {
    onUpdate('default_provider', v);
    if (v === 'ollama') {
      onUpdate('default_model', '');
      return;
    }
    const models = MODELS_BY_PROVIDER[v];
    if (models && models.length > 0) {
      onUpdate('default_model', models[0]!.value);
    }
  };

  const handleOllamaModelSelect = async (model: string) => {
    onUpdate('default_provider', 'ollama');
    onUpdate('default_model', model);
    await onCommit([
      { path: 'default_provider', value: 'ollama' },
      { path: 'default_model', value: model },
    ]);
  };

  const handlePullModel = async () => {
    const model = pullModelName.trim();
    if (!model) return;
    setPullingModel(true);
    setPullMessage(null);
    try {
      const result = await pullOllamaModel(model);
      setPullMessage(result.message);
      setPullModelName('');
      window.setTimeout(() => {
        loadOllamaModels();
      }, 3000);
    } catch (error: unknown) {
      setPullMessage(error instanceof Error ? error.message : 'Failed to start Ollama pull');
    } finally {
      setPullingModel(false);
    }
  };

  return (
    <SectionCard
      icon={<Zap className="h-5 w-5" />}
      title={t('config.section.general')}
      defaultOpen
    >
      <FieldRow label={t('config.field.default_provider')} description={t('config.field.default_provider.desc')}>
        <Select
          value={provider}
          onChange={handleProviderChange}
          options={PROVIDER_OPTIONS}
        />
      </FieldRow>
      <FieldRow label={t('config.field.default_model')} description={t('config.field.default_model.desc')}>
        {provider === 'ollama' ? (
          <div className="flex flex-col items-end gap-2">
            <div className="flex items-center gap-2">
              {modelOptions && modelOptions.length > 0 ? (
                <Select
                  value={modelOptions.some((o) => o.value === currentModel) ? currentModel : modelOptions[0]?.value ?? ''}
                  onChange={handleOllamaModelSelect}
                  options={modelOptions}
                  disabled={loadingOllama}
                />
              ) : (
                <input
                  type="text"
                  value={currentModel}
                  onChange={(e) => onUpdate('default_model', e.target.value)}
                  placeholder={loadingOllama ? 'detecting local models...' : 'enter local ollama model'}
                  className="input-electric text-sm px-3 py-1.5 w-64 font-mono"
                />
              )}
              <button
                type="button"
                onClick={loadOllamaModels}
                disabled={loadingOllama}
                className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium border transition-colors"
                style={{ borderColor: 'var(--pc-border)', color: 'var(--pc-text-secondary)', background: 'var(--pc-bg-surface)' }}
              >
                {loadingOllama ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <RotateCw className="h-3.5 w-3.5" />}
                Refresh
              </button>
            </div>
            <div className="text-[11px] max-w-80 text-right" style={{ color: ollamaReachable === false ? 'var(--color-status-error)' : 'var(--pc-text-muted)' }}>
              {loadingOllama
                ? 'Detecting local Ollama models...'
                : modelOptions && modelOptions.length > 0
                  ? `Detected ${modelOptions.length} local model${modelOptions.length === 1 ? '' : 's'} from Ollama.`
                  : ollamaError
                    ? ollamaError
                    : 'Ollama is reachable, but no local models were returned. Pull a model first or enter one manually.'}
            </div>
            {(!modelOptions || modelOptions.length === 0) && (
              <div className="flex flex-col items-end gap-2 w-full max-w-80">
                <div className="flex items-center gap-2 w-full justify-end">
                  <input
                    type="text"
                    value={pullModelName}
                    onChange={(e) => setPullModelName(e.target.value)}
                    placeholder="e.g. qwen3.5:35b"
                    className="input-electric text-sm px-3 py-1.5 w-52 font-mono"
                  />
                  <button
                    type="button"
                    onClick={handlePullModel}
                    disabled={pullingModel || !pullModelName.trim()}
                    className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium border transition-colors disabled:opacity-50"
                    style={{ borderColor: 'var(--pc-border)', color: 'var(--pc-text-secondary)', background: 'var(--pc-bg-surface)' }}
                  >
                    {pullingModel ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <ArrowDownToLine className="h-3.5 w-3.5" />}
                    Pull Model
                  </button>
                </div>
                {pullMessage && (
                  <div className="text-[11px] max-w-80 text-right" style={{ color: 'var(--pc-text-muted)' }}>
                    {pullMessage}
                  </div>
                )}
              </div>
            )}
          </div>
        ) : modelOptions ? (
          <Select
            value={modelOptions.some((o) => o.value === currentModel) ? currentModel : ''}
            onChange={(v) => onUpdate('default_model', v)}
            options={[
              ...(currentModel && !modelOptions.some((o) => o.value === currentModel)
                ? [{ value: currentModel, label: currentModel }]
                : []),
              ...modelOptions,
            ]}
          />
        ) : (
          <input
            type="text"
            value={currentModel}
            onChange={(e) => onUpdate('default_model', e.target.value)}
            placeholder="model name"
            className="input-electric text-sm px-3 py-1.5 w-52 font-mono"
          />
        )}
      </FieldRow>
      <FieldRow label={t('config.field.default_temperature')} description={t('config.field.default_temperature.desc')}>
        <Slider
          value={(config.default_temperature as number) ?? 0.7}
          onChange={(v) => onUpdate('default_temperature', v)}
          min={0}
          max={2}
          step={0.1}
        />
      </FieldRow>
      <FieldRow label={t('config.field.provider_timeout_secs')} description={t('config.field.provider_timeout_secs.desc')}>
        <NumberInput
          value={(config.provider_timeout_secs as number) ?? 120}
          onChange={(v) => onUpdate('provider_timeout_secs', v)}
          min={1}
        />
      </FieldRow>
      <FieldRow label={t('config.field.locale')} description={t('config.field.locale.desc')}>
        <Select
          value={(config.locale as string) ?? ''}
          onChange={(v) => onUpdate('locale', v || undefined)}
          options={LOCALE_OPTIONS}
        />
      </FieldRow>
    </SectionCard>
  );
}
