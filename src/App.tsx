import { useEffect, useState, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import "./App.css";

const SIZE_PICKER = new LogicalSize(320, 400);
const SIZE_DETAIL = new LogicalSize(250, 500);
const SIZE_DETAIL_MINI = new LogicalSize(64, 350);
const SIZE_SETTINGS = new LogicalSize(340, 500);

function startDrag(e: React.MouseEvent) {
  e.preventDefault();
  getCurrentWindow().startDragging();
}

interface TimerDef {
  id: string;
  name: string;
  icon: string;
  duration_secs: number;
  hotkey: string | null;
  color: string;
  warning_secs: number;
  repeat: boolean;
}

interface BossConfig {
  boss: { name: string; description: string };
  timers: TimerDef[];
}

interface Timer {
  id: string;
  name: string;
  icon: string;
  duration: number;
  remaining: number;
  state: "Running" | "Warning" | "Expired";
  color: string;
  warning_secs: number;
  def_id: string;
}

interface TimerUpdate {
  timers: Timer[];
}

interface BossListItem {
  id: string;
  name: string;
  description: string;
  timer_count: number;
}

interface AppSettings {
  back_hotkey: string;
  stop_all_hotkey: string;
  hotkeys: Record<string, Record<string, string>>;
  hidden_timers: Record<string, string[]>;
  mini_mode: boolean;
}

interface BossHotkeyInfo {
  timer_id: string;
  timer_name: string;
  default_hotkey: string | null;
  effective_hotkey: string | null;
}

interface SelectBossResponse {
  config: BossConfig;
  hidden_timers: string[];
  muted_timers: string[];
  mini_mode: boolean;
}

// --- MechanicRow: full-size mechanic display ---
function MechanicRow({
  def,
  timer,
  effectiveHotkey,
  isMuted,
  onHide,
  onToggleMute,
  miniMode,
}: {
  def: TimerDef;
  timer: Timer | undefined;
  effectiveHotkey: string | null;
  isMuted: boolean;
  onHide: () => void;
  onToggleMute: () => void;
  miniMode: boolean;
}) {
  const isActive = !!timer;
  const remaining = timer ? Math.max(0, timer.remaining) : def.duration_secs;
  const secs = Math.ceil(remaining);
  const progress = def.duration_secs > 0 ? remaining / def.duration_secs : 0;

  const stateClass = timer
    ? timer.state === "Expired"
      ? "mechanic-expired"
      : timer.state === "Warning"
        ? "mechanic-warning"
        : "mechanic-active"
    : "mechanic-idle";

  return (
    <div className={`mechanic-row ${stateClass}`}>
      <span className="mechanic-icon">{def.icon}</span>
      <div className="mechanic-body">
        <div className="mechanic-header">
          <span className="mechanic-name">{def.name}</span>
          <div className="mechanic-right">
            {effectiveHotkey && (
              <span className="mechanic-hotkey">{effectiveHotkey}</span>
            )}
            <button
              className={`mechanic-mute-btn ${isMuted ? "mechanic-muted" : ""}`}
              onClick={onToggleMute}
              title={isMuted ? "取消靜音 / Unmute" : "靜音 / Mute"}
            >
              {isMuted ? "🔇" : "🔊"}
            </button>
            {!miniMode && (
              <button className="mechanic-hide-btn" onClick={onHide}>
                ✕
              </button>
            )}
          </div>
        </div>
        <div className="mechanic-bar-bg">
          <div
            className="mechanic-bar-fill"
            style={{
              width: `${progress * 100}%`,
              backgroundColor: isActive ? def.color : "rgba(255,255,255,0.15)",
            }}
          />
        </div>
        <span className="mechanic-time">
          {timer?.state === "Expired" ? "TIME!" : `${secs}s`}
        </span>
      </div>
    </div>
  );
}

// --- MiniTile: 32x32 buff icon like MapleStory buff bar ---
function MiniTile({
  def,
  timer,
}: {
  def: TimerDef;
  timer: Timer | undefined;
}) {
  const remaining = timer ? Math.max(0, timer.remaining) : def.duration_secs;
  const secs = Math.ceil(remaining);
  const progress = def.duration_secs > 0 ? remaining / def.duration_secs : 1;
  // Clockwise sweep: dark covers elapsed time, clear = remaining
  const elapsedAngle = (1 - progress) * 360;

  const stateClass = timer
    ? timer.state === "Expired"
      ? "buff-expired"
      : timer.state === "Warning"
        ? "buff-warning"
        : "buff-active"
    : "buff-idle";

  return (
    <div className={`buff-icon ${stateClass}`}>
      <span className="buff-emoji">{def.icon}</span>
      <span className="buff-secs">
        {timer?.state === "Expired" ? "!" : `${secs}`}
      </span>
      {timer && timer.state !== "Expired" && (
        <div
          className="buff-sweep"
          style={{
            background: `conic-gradient(rgba(0,0,0,0.6) ${elapsedAngle}deg, transparent ${elapsedAngle}deg)`,
          }}
        />
      )}
    </div>
  );
}

// --- BossDetailPage ---
function BossDetailPage({
  config,
  timers,
  hiddenTimers,
  mutedTimers,
  miniMode,
  hotkeys,
  onBack,
  onHideTimer,
  onToggleMute,
  onResetHidden,
  onToggleMini,
}: {
  config: BossConfig;
  timers: Timer[];
  hiddenTimers: string[];
  mutedTimers: string[];
  miniMode: boolean;
  hotkeys: Record<string, string>;
  onBack: () => void;
  onHideTimer: (timerId: string) => void;
  onToggleMute: (timerId: string) => void;
  onResetHidden: () => void;
  onToggleMini: () => void;
}) {
  const visibleDefs = config.timers.filter(
    (d) => !hiddenTimers.includes(d.id)
  );

  // Map running timers to their defs (prefer non-expired over expired)
  const timerByDef: Record<string, Timer> = {};
  for (const t of timers) {
    const existing = timerByDef[t.def_id];
    if (!existing || (existing.state === "Expired" && t.state !== "Expired")) {
      timerByDef[t.def_id] = t;
    }
  }

  // --- Mini mode: single-column buff bar ---
  if (miniMode) {
    return (
      <div className="buff-bar">
        <div className="buff-bar-drag" onMouseDown={startDrag}>
          <span className="buff-bar-grip">⋮</span>
        </div>
        {visibleDefs.map((def) => (
          <MiniTile key={def.id} def={def} timer={timerByDef[def.id]} />
        ))}
        <div className="buff-bar-actions">
          <button
            className="buff-bar-btn"
            onClick={onToggleMini}
            onMouseDown={(e) => e.stopPropagation()}
            title="展開 / Expand"
          >
            ⊟
          </button>
        </div>
      </div>
    );
  }

  // --- Normal mode: transparent HUD overlay ---
  return (
    <div className="hud-detail">
      <div className="hud-header" onMouseDown={startDrag}>
        <span className="hud-boss-name">{config.boss.name}</span>
        <div className="hud-actions">
          {hiddenTimers.length > 0 && (
            <button
              className="hud-btn"
              onClick={onResetHidden}
              onMouseDown={(e) => e.stopPropagation()}
              title="重置隱藏 / Reset Hidden"
            >
              ↺
            </button>
          )}
          <button
            className="hud-btn"
            onClick={onToggleMini}
            onMouseDown={(e) => e.stopPropagation()}
            title="迷你模式 / Mini Mode"
          >
            ⊞
          </button>
          <button
            className="hud-btn hud-btn-close"
            onClick={onBack}
            onMouseDown={(e) => e.stopPropagation()}
          >
            ✕
          </button>
        </div>
      </div>
      <div className="mechanic-list">
        {visibleDefs.map((def) => (
          <MechanicRow
            key={def.id}
            def={def}
            timer={timerByDef[def.id]}
            effectiveHotkey={hotkeys[def.id] || def.hotkey}
            isMuted={mutedTimers.includes(def.id)}
            onHide={() => onHideTimer(def.id)}
            onToggleMute={() => onToggleMute(def.id)}
            miniMode={miniMode}
          />
        ))}
      </div>
    </div>
  );
}

// --- HotkeyCapture ---
function buildHotkeyString(e: React.KeyboardEvent): string | null {
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");

  const key = e.key;
  if (["Control", "Shift", "Alt", "Meta"].includes(key)) {
    return null;
  }

  const keyMap: Record<string, string> = {
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    " ": "Space",
    "`": "Backquote",
  };

  const mappedKey = keyMap[key] || (key.length === 1 ? key.toUpperCase() : key);
  parts.push(mappedKey);

  return parts.join("+");
}

function HotkeyCapture({
  currentHotkey,
  onCapture,
  onCancel,
}: {
  currentHotkey: string | null;
  onCapture: (hotkey: string) => void;
  onCancel: () => void;
}) {
  const [capturing, setCapturing] = useState(false);

  if (!capturing) {
    return (
      <span className="hotkey-display" onClick={() => setCapturing(true)}>
        {currentHotkey || "未設定"}
      </span>
    );
  }

  return (
    <span
      className="hotkey-capture"
      tabIndex={0}
      autoFocus
      onKeyDown={(e) => {
        e.preventDefault();
        e.stopPropagation();
        if (e.key === "Escape") {
          setCapturing(false);
          onCancel();
          return;
        }
        const hotkey = buildHotkeyString(e);
        if (hotkey) {
          setCapturing(false);
          onCapture(hotkey);
        }
      }}
      onBlur={() => {
        setCapturing(false);
        onCancel();
      }}
    >
      按下快捷鍵...
    </span>
  );
}

// --- SettingsPage ---
function SettingsPage({
  bosses,
  onBack,
}: {
  bosses: BossListItem[];
  onBack: () => void;
}) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [bossHotkeys, setBossHotkeys] = useState<Record<string, BossHotkeyInfo[]>>({});
  useEffect(() => {
    invoke<AppSettings>("get_settings").then(setSettings);
    Promise.all(
      bosses.map((b) =>
        invoke<BossHotkeyInfo[]>("get_boss_hotkeys", { bossId: b.id }).then(
          (hotkeys) => ({ bossId: b.id, hotkeys })
        )
      )
    ).then((results) => {
      const map: Record<string, BossHotkeyInfo[]> = {};
      for (const r of results) {
        map[r.bossId] = r.hotkeys;
      }
      setBossHotkeys(map);
    });
  }, [bosses]);

  const persistSettings = async (newSettings: AppSettings) => {
    setSettings(newSettings);
    await invoke("save_settings", { payload: { settings: newSettings } });
  };

  const updateGlobalHotkey = (field: "back_hotkey" | "stop_all_hotkey", value: string) => {
    if (!settings) return;
    persistSettings({ ...settings, [field]: value });
  };

  const updateTimerHotkey = (bossId: string, timerId: string, hotkey: string) => {
    if (!settings) return;
    const newHotkeys = { ...settings.hotkeys };
    if (!newHotkeys[bossId]) {
      newHotkeys[bossId] = {};
    }
    newHotkeys[bossId] = { ...newHotkeys[bossId], [timerId]: hotkey };
    persistSettings({ ...settings, hotkeys: newHotkeys });

    const newBossHotkeys = { ...bossHotkeys };
    if (newBossHotkeys[bossId]) {
      newBossHotkeys[bossId] = newBossHotkeys[bossId].map((h) =>
        h.timer_id === timerId ? { ...h, effective_hotkey: hotkey } : h
      );
      setBossHotkeys(newBossHotkeys);
    }
  };

  if (!settings) return null;

  return (
    <div className="settings-page">
      <div className="window-header" onMouseDown={startDrag}>
        <div className="picker-title">快捷鍵設定</div>
        <button className="close-btn" onClick={() => getCurrentWindow().close()} onMouseDown={(e) => e.stopPropagation()}>
          ✕
        </button>
      </div>

      <div className="settings-scroll">
        <div className="picker-subtitle">Hotkey Settings</div>

        <div className="settings-section">
          <div className="settings-section-title">全局 / Global</div>
          <div className="hotkey-row">
            <span className="hotkey-label">返回主畫面</span>
            <HotkeyCapture
              currentHotkey={settings.back_hotkey}
              onCapture={(hk) => updateGlobalHotkey("back_hotkey", hk)}
              onCancel={() => {}}
            />
          </div>
          <div className="hotkey-row">
            <span className="hotkey-label">停止全部</span>
            <HotkeyCapture
              currentHotkey={settings.stop_all_hotkey}
              onCapture={(hk) => updateGlobalHotkey("stop_all_hotkey", hk)}
              onCancel={() => {}}
            />
          </div>
        </div>

        {bosses.map((boss) => (
          <div key={boss.id} className="settings-section">
            <div className="settings-section-title">{boss.name}</div>
            {(bossHotkeys[boss.id] || []).map((hk) => (
              <div key={hk.timer_id} className="hotkey-row">
                <span className="hotkey-label">{hk.timer_name}</span>
                <HotkeyCapture
                  currentHotkey={hk.effective_hotkey}
                  onCapture={(hotkey) => updateTimerHotkey(boss.id, hk.timer_id, hotkey)}
                  onCancel={() => {}}
                />
              </div>
            ))}
          </div>
        ))}
      </div>

      <div className="settings-footer">
        <button className="settings-btn back-btn" onClick={onBack}>
          返回 / Back
        </button>
      </div>
    </div>
  );
}

// --- App ---
function App() {
  const [timers, setTimers] = useState<Timer[]>([]);
  const [activeBoss, setActiveBoss] = useState<string | null>(null);
  const [bossConfig, setBossConfig] = useState<BossConfig | null>(null);
  const [hiddenTimers, setHiddenTimers] = useState<string[]>([]);
  const [mutedTimers, setMutedTimers] = useState<string[]>([]);
  const [miniMode, setMiniMode] = useState(false);
  const [bosses, setBosses] = useState<BossListItem[]>([]);
  const [showPicker, setShowPicker] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const [hotkeyOverrides, setHotkeyOverrides] = useState<Record<string, string>>({});

  // Load boss list on mount
  useEffect(() => {
    invoke<BossListItem[]>("list_bosses").then(setBosses);
  }, []);

  // Listen for timer updates
  useEffect(() => {
    const unlisten = listen<TimerUpdate>("timer-update", (event) => {
      const sorted = [...event.payload.timers].sort((a, b) => {
        if (a.state === "Expired" && b.state !== "Expired") return 1;
        if (a.state !== "Expired" && b.state === "Expired") return -1;
        return a.remaining - b.remaining;
      });
      setTimers(sorted);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Listen for back-to-main event from Rust (hotkey)
  useEffect(() => {
    const unlisten = listen("back-to-main", () => {
      setActiveBoss(null);
      setBossConfig(null);
      setShowPicker(true);
      setShowSettings(false);
      setTimers([]);
      getCurrentWindow().setSize(SIZE_PICKER);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const selectBoss = async (bossId: string) => {
    const resp = await invoke<SelectBossResponse>("select_boss", { bossId });
    setActiveBoss(bossId);
    setBossConfig(resp.config);
    setHiddenTimers(resp.hidden_timers);
    setMutedTimers(resp.muted_timers);
    setMiniMode(resp.mini_mode);
    setShowPicker(false);
    setTimers([]);

    // Load hotkey overrides
    const settings = await invoke<AppSettings>("get_settings");
    setHotkeyOverrides(settings.hotkeys[bossId] || {});

    getCurrentWindow().setSize(resp.mini_mode ? SIZE_DETAIL_MINI : SIZE_DETAIL);
  };

  const goBackToMain = useCallback(async () => {
    await invoke("stop_all_timers");
    setActiveBoss(null);
    setBossConfig(null);
    setShowPicker(true);
    setShowSettings(false);
    setTimers([]);
    getCurrentWindow().setSize(SIZE_PICKER);
  }, []);

  const handleHideTimer = async (timerId: string) => {
    if (!activeBoss) return;
    await invoke("hide_timer", { bossId: activeBoss, timerId });
    setHiddenTimers((prev) => [...prev, timerId]);
  };

  const handleResetHidden = async () => {
    if (!activeBoss) return;
    await invoke("reset_hidden_timers", { bossId: activeBoss });
    setHiddenTimers([]);
  };

  const handleToggleMute = async (timerId: string) => {
    if (!activeBoss) return;
    const isMuted = await invoke<boolean>("toggle_mute_timer", { bossId: activeBoss, timerId });
    setMutedTimers((prev) =>
      isMuted ? [...prev, timerId] : prev.filter((id) => id !== timerId)
    );
  };

  const handleToggleMini = async () => {
    const newMode = !miniMode;
    await invoke("set_mini_mode", { enabled: newMode });
    setMiniMode(newMode);
    getCurrentWindow().setSize(newMode ? SIZE_DETAIL_MINI : SIZE_DETAIL);
  };

  const closeApp = () => {
    getCurrentWindow().close();
  };

  // Show settings page
  if (showSettings) {
    return (
      <SettingsPage
        bosses={bosses}
        onBack={() => {
          setShowSettings(false);
          getCurrentWindow().setSize(SIZE_PICKER);
        }}
      />
    );
  }

  // Show boss detail page when a boss is selected
  if (activeBoss && bossConfig) {
    return (
      <BossDetailPage
        config={bossConfig}
        timers={timers}
        hiddenTimers={hiddenTimers}
        mutedTimers={mutedTimers}
        miniMode={miniMode}
        hotkeys={hotkeyOverrides}
        onBack={goBackToMain}
        onHideTimer={handleHideTimer}
        onToggleMute={handleToggleMute}
        onResetHidden={handleResetHidden}
        onToggleMini={handleToggleMini}
      />
    );
  }

  // Show boss picker when no boss is selected
  if (showPicker && !activeBoss) {
    return (
      <div className="boss-picker">
        <div className="window-header" onMouseDown={startDrag}>
          <div className="picker-title">Artale Timer</div>
          <button className="close-btn" onClick={closeApp} onMouseDown={(e) => e.stopPropagation()}>✕</button>
        </div>
        <div className="picker-subtitle">選擇 Boss / Select Boss</div>
        <div className="boss-list">
          {bosses.map((boss) => (
            <button
              key={boss.id}
              className="boss-button"
              onClick={() => selectBoss(boss.id)}
            >
              <span className="boss-name">{boss.name}</span>
              <span className="boss-desc">{boss.description}</span>
              <span className="boss-timers">{boss.timer_count} timers</span>
            </button>
          ))}
        </div>
        <button
          className="settings-link"
          onClick={() => {
            setShowSettings(true);
            getCurrentWindow().setSize(SIZE_SETTINGS);
          }}
        >
          快捷鍵設定 / Hotkey Settings
        </button>
      </div>
    );
  }

  return null;
}

export default App;
