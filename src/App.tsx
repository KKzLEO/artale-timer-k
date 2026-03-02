import { useEffect, useState, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import "./App.css";

const SIZE_PICKER = new LogicalSize(320, 400);
const SIZE_OVERLAY = new LogicalSize(270, 500);
const SIZE_SETTINGS = new LogicalSize(340, 500);

function startDrag(e: React.MouseEvent) {
  e.preventDefault();
  getCurrentWindow().startDragging();
}

interface Timer {
  id: string;
  name: string;
  duration: number;
  remaining: number;
  state: "Running" | "Warning" | "Expired";
  color: string;
  warning_secs: number;
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
}

interface BossHotkeyInfo {
  timer_id: string;
  timer_name: string;
  default_hotkey: string | null;
  effective_hotkey: string | null;
}

function TimerDisplay({ timer }: { timer: Timer }) {
  const remaining = Math.max(0, timer.remaining);
  const secs = Math.ceil(remaining);
  const progress = timer.duration > 0 ? remaining / timer.duration : 0;

  const stateClass =
    timer.state === "Expired"
      ? "timer-expired"
      : timer.state === "Warning"
        ? "timer-warning"
        : "timer-running";

  return (
    <div className={`timer-item ${stateClass}`}>
      <div className="timer-bar-bg">
        <div
          className="timer-bar-fill"
          style={{
            width: `${progress * 100}%`,
            backgroundColor: timer.color,
          }}
        />
      </div>
      <div className="timer-info">
        <span className="timer-name">{timer.name}</span>
        <span className="timer-time">
          {timer.state === "Expired" ? "TIME!" : `${secs}s`}
        </span>
      </div>
    </div>
  );
}

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

  // Auto-save: persist settings to disk immediately on change
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

function App() {
  const [timers, setTimers] = useState<Timer[]>([]);
  const [activeBoss, setActiveBoss] = useState<string | null>(null);
  const [bosses, setBosses] = useState<BossListItem[]>([]);
  const [showPicker, setShowPicker] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
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
    await invoke("select_boss", { bossId });
    setActiveBoss(bossId);
    setShowPicker(false);
    getCurrentWindow().setSize(SIZE_OVERLAY);
  };

  const goBackToMain = useCallback(async () => {
    await invoke("stop_all_timers");
    setActiveBoss(null);
    setShowPicker(true);
    setShowSettings(false);
    setTimers([]);
    getCurrentWindow().setSize(SIZE_PICKER);
  }, []);

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
        <div className="picker-hint">
          選擇後按快捷鍵觸發計時器，Alt+Home 返回主畫面
        </div>
      </div>
    );
  }

  // Show overlay with drag handle + timers
  return (
    <div className="overlay-container">
      <div className="overlay-drag-handle" onMouseDown={startDrag}>
        <span className="drag-dots">⋮⋮</span>
      </div>
      {activeBoss && timers.length === 0 && (
        <div className="status-badge" onClick={goBackToMain}>
          <span className="status-dot" />
          <span className="status-text">
            {bosses.find((b) => b.id === activeBoss)?.name ?? activeBoss} - 等待觸發
          </span>
        </div>
      )}
      <div className="timer-list">
        {timers.map((timer) => (
          <TimerDisplay key={timer.id} timer={timer} />
        ))}
      </div>
    </div>
  );
}

export default App;
