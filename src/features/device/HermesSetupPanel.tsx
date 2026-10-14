import { useState, type FormEvent } from "react";
import { Button } from "../../components/Button";
import { ipc } from "../../lib/ipc";
import type { ErrorPayload, HermesSetupInput } from "../../types";

/** Provider ids the backend maps to setup env keys; "custom" uses config.yaml. */
const PROVIDERS = [
  "openrouter",
  "openai",
  "anthropic",
  "gemini",
  "nous-api",
  "kimi-coding",
  "minimax",
  "zai",
  "ollama-cloud",
  "fireworks",
  "custom",
] as const;

const FIELD =
  "w-full rounded-md border border-border bg-bg px-2 py-1.5 font-mono text-[12px] text-text";
const LABEL = "text-[12px] text-muted";

function emptyToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

interface HermesSetupPanelProps {
  serial: string;
  recipeId: string;
  onApplied: () => void;
}

/**
 * In-app replacement for the manual `hermes setup` / `hermes gateway setup`
 * terminal handoff: collects the same input and applies it over SSH via the
 * backend `apply_hermes_setup` command (env writes + `hermes config set`).
 */
export function HermesSetupPanel({ serial, recipeId, onApplied }: HermesSetupPanelProps) {
  const [modelProvider, setModelProvider] = useState("");
  const [modelName, setModelName] = useState("");
  const [modelBaseUrl, setModelBaseUrl] = useState("");
  const [providerApiKey, setProviderApiKey] = useState("");
  const [telegramBotToken, setTelegramBotToken] = useState("");
  const [telegramAllowedUsers, setTelegramAllowedUsers] = useState("");
  const [discordBotToken, setDiscordBotToken] = useState("");
  const [discordAllowedUsers, setDiscordAllowedUsers] = useState("");
  const [slackAppToken, setSlackAppToken] = useState("");
  const [slackBotToken, setSlackBotToken] = useState("");
  const [slackAllowedUsers, setSlackAllowedUsers] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [applyError, setApplyError] = useState<ErrorPayload | null>(null);
  const [applying, setApplying] = useState(false);

  function buildInput(): HermesSetupInput {
    return {
      modelProvider: emptyToNull(modelProvider),
      modelName: emptyToNull(modelName),
      modelBaseUrl: emptyToNull(modelBaseUrl),
      providerApiKey: emptyToNull(providerApiKey),
      telegramBotToken: emptyToNull(telegramBotToken),
      telegramAllowedUsers: emptyToNull(telegramAllowedUsers),
      discordBotToken: emptyToNull(discordBotToken),
      discordAllowedUsers: emptyToNull(discordAllowedUsers),
      slackAppToken: emptyToNull(slackAppToken),
      slackBotToken: emptyToNull(slackBotToken),
      slackAllowedUsers: emptyToNull(slackAllowedUsers),
    };
  }

  function validate(input: HermesSetupInput): string | null {
    const hasProvider = input.modelProvider !== null;
    const hasMessaging =
      input.telegramBotToken !== null ||
      input.telegramAllowedUsers !== null ||
      input.discordBotToken !== null ||
      input.discordAllowedUsers !== null ||
      input.slackAppToken !== null ||
      input.slackBotToken !== null ||
      input.slackAllowedUsers !== null;
    if (!hasProvider && !hasMessaging) {
      return "Enter a model provider with its API key, or a messaging platform token with its allow-list.";
    }
    if (hasProvider && input.providerApiKey === null) {
      return "The provider API key is required for the selected model provider.";
    }
    if (input.telegramBotToken !== null && input.telegramAllowedUsers === null) {
      return "Telegram allowed users are required with a bot token.";
    }
    if (input.discordBotToken !== null && input.discordAllowedUsers === null) {
      return "Discord allowed users are required with a bot token.";
    }
    if (
      input.slackBotToken !== null &&
      (input.slackAppToken === null || input.slackAllowedUsers === null)
    ) {
      return "Slack app token and allowed users are required with a bot token.";
    }
    if (input.slackAppToken !== null && input.slackBotToken === null) {
      return "Slack bot token is required with an app token.";
    }
    return null;
  }

  async function apply(event: FormEvent) {
    event.preventDefault();
    setFormError(null);
    setApplyError(null);
    const input = buildInput();
    const problem = validate(input);
    if (problem) {
      setFormError(problem);
      return;
    }
    setApplying(true);
    try {
      await ipc.applyHermesSetup(serial, recipeId, input);
      onApplied();
    } catch (e) {
      setApplyError(e as ErrorPayload);
    } finally {
      setApplying(false);
    }
  }

  return (
    <form onSubmit={(event) => void apply(event)} className="flex flex-col gap-3">
      <h3 className="text-[13px] font-semibold">Configure Hermes automatically</h3>
      <p className="text-[12px] text-muted">
        Applied over SSH to the phone&rsquo;s Hermes environment. Secrets are written only to{" "}
        <code className="font-mono">~/.hermes/.env</code> on the phone (mode 600), never stored on
        this computer.
      </p>

      <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
        <label className={`flex flex-col gap-1 ${LABEL}`}>
          Model provider
          <select
            aria-label="Model provider"
            className={FIELD}
            value={modelProvider}
            onChange={(event) => setModelProvider(event.target.value)}
          >
            <option value="">Select a provider…</option>
            {PROVIDERS.map((provider) => (
              <option key={provider} value={provider}>
                {provider}
              </option>
            ))}
          </select>
        </label>
        <label className={`flex flex-col gap-1 ${LABEL}`}>
          Model name
          <input
            aria-label="Model name"
            className={FIELD}
            value={modelName}
            onChange={(event) => setModelName(event.target.value)}
          />
        </label>
        <label className={`flex flex-col gap-1 ${LABEL}`}>
          Model base URL (optional)
          <input
            aria-label="Model base URL"
            className={FIELD}
            value={modelBaseUrl}
            onChange={(event) => setModelBaseUrl(event.target.value)}
          />
        </label>
        <label className={`flex flex-col gap-1 ${LABEL}`}>
          Provider API key
          <input
            aria-label="Provider API key"
            type="password"
            autoComplete="off"
            className={FIELD}
            value={providerApiKey}
            onChange={(event) => setProviderApiKey(event.target.value)}
          />
        </label>
      </div>

      <fieldset className="flex flex-col gap-2 rounded-md border border-border p-3">
        <legend className="px-1 text-[12px] font-semibold">
          Messaging (optional — keep direct messages allow-listed)
        </legend>
        <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
          <label className={`flex flex-col gap-1 ${LABEL}`}>
            Telegram bot token
            <input
              aria-label="Telegram bot token"
              type="password"
              autoComplete="off"
              className={FIELD}
              value={telegramBotToken}
              onChange={(event) => setTelegramBotToken(event.target.value)}
            />
          </label>
          <label className={`flex flex-col gap-1 ${LABEL}`}>
            Telegram allowed users
            <input
              aria-label="Telegram allowed users"
              className={FIELD}
              value={telegramAllowedUsers}
              onChange={(event) => setTelegramAllowedUsers(event.target.value)}
            />
          </label>
          <label className={`flex flex-col gap-1 ${LABEL}`}>
            Discord bot token
            <input
              aria-label="Discord bot token"
              type="password"
              autoComplete="off"
              className={FIELD}
              value={discordBotToken}
              onChange={(event) => setDiscordBotToken(event.target.value)}
            />
          </label>
          <label className={`flex flex-col gap-1 ${LABEL}`}>
            Discord allowed users
            <input
              aria-label="Discord allowed users"
              className={FIELD}
              value={discordAllowedUsers}
              onChange={(event) => setDiscordAllowedUsers(event.target.value)}
            />
          </label>
          <label className={`flex flex-col gap-1 ${LABEL}`}>
            Slack app token
            <input
              aria-label="Slack app token"
              type="password"
              autoComplete="off"
              className={FIELD}
              value={slackAppToken}
              onChange={(event) => setSlackAppToken(event.target.value)}
            />
          </label>
          <label className={`flex flex-col gap-1 ${LABEL}`}>
            Slack bot token
            <input
              aria-label="Slack bot token"
              type="password"
              autoComplete="off"
              className={FIELD}
              value={slackBotToken}
              onChange={(event) => setSlackBotToken(event.target.value)}
            />
          </label>
          <label className={`flex flex-col gap-1 ${LABEL}`}>
            Slack allowed users
            <input
              aria-label="Slack allowed users"
              className={FIELD}
              value={slackAllowedUsers}
              onChange={(event) => setSlackAllowedUsers(event.target.value)}
            />
          </label>
        </div>
      </fieldset>

      <p className="text-[12px] text-muted">
        Inside proot-distro there is no systemd: this app supervises the gateway itself, so no
        service is installed or started.
      </p>

      {formError && (
        <p role="alert" className="text-[12px] text-warning">
          {formError}
        </p>
      )}
      {applyError && (
        <p role="alert" className="whitespace-pre-wrap text-[12px] text-danger">
          {applyError.message}
        </p>
      )}

      <span>
        <Button type="submit" variant="primary" loading={applying} disabled={applying}>
          Apply setup
        </Button>
      </span>
    </form>
  );
}
