import {
  MicPanel,
  MicTile,
  ProgressPanel,
  ProgressTile,
  RecycleBinPanel,
  RecycleBinTile,
  ShowDesktopPanel,
  ShowDesktopTile,
  StoragePanel,
  StorageTile,
  ThemePanel,
  ThemeTile,
  TopAppsPanel,
  TopAppsTile,
} from "./gadgets";
import * as C from "./clocks";
import * as P from "./pc";
import * as T from "./tools";
import { AiLimitsPanel, AiLimitsTile, DEFS, serviceWidget, TotalRevenuePanel, TotalRevenueTile } from "./services";
import {
  HourlyTile,
  SunTile,
  TodayForecastTile,
  WeatherDetailsTile,
  WeatherStatusTile,
} from "./weather";
import type { ComponentType } from "react";
import type { WidgetProps, WidgetSize } from "./shared";
import { SystemPanel, SystemTile } from "./system";
import { WeatherPanel, WeatherTile } from "./weather";
import { TodoPanel, TodoTile } from "./todo";
import { NotesPanel, NotesTile } from "./notes";
import { PomodoroPanel, PomodoroTile } from "./pomodoro";
import { WorldClockPanel, WorldClockTile } from "./worldclock";
import { AiPanel, AiTile, aiWidget } from "./ai";
import {
  BatteryPanel,
  BatteryTile,
  NowPlayingPanel,
  NowPlayingTile,
  ScreenTimePanel,
  ScreenTimeTile,
  SpeakersPanel,
  SpeakersTile,
  TogglesPanel,
  TogglesTile,
  WallpaperPanel,
  WallpaperTile,
} from "./device";

/**
 * A widget = this manifest + a Tile (in the dock) + a Panel (the expanded view
 * in the popover) + optional Rust data commands.
 */
export const GROUPS = [
  "AI",
  "Analytics",
  "Apps & Files",
  "Clocks",
  "Developer",
  "Finance",
  "Media",
  "Online & Mail",
  "Productivity",
  "Social",
  "Stocks",
  "System",
  "Weather",
] as const;
type Group = (typeof GROUPS)[number];

export type WidgetKind = {
  kind: string;
  name: string;
  group: Group;
  description: string;
  sizes: WidgetSize[];
  defaultSize: WidgetSize;
  /** Expanded view size in CSS px. */
  panel: { width: number; height: number };
  /** Kept for existing widgets but not offered in the picker. */
  hidden?: boolean;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  Tile: ComponentType<WidgetProps<any>>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  Panel: ComponentType<WidgetProps<any>>;
};

export const WIDGETS: WidgetKind[] = [
  {
    kind: "battery",
    name: "Battery",
    group: "System",
    description: "Charge level, charging, and time left.",
    sizes: ["s", "m"],
    defaultSize: "s",
    panel: { width: 300, height: 220 },
    Tile: BatteryTile,
    Panel: BatteryPanel,
  },
  {
    kind: "toggles",
    name: "Quick toggles",
    group: "System",
    description: "Wi-Fi and Bluetooth; hotspot and airplane mode in the details.",
    sizes: ["m", "l"],
    defaultSize: "m",
    panel: { width: 300, height: 320 },
    Tile: TogglesTile,
    Panel: TogglesPanel,
  },
  {
    kind: "topapps",
    name: "Top apps",
    group: "System",
    description: "The apps using the most CPU and memory right now.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 320, height: 380 },
    Tile: TopAppsTile,
    Panel: TopAppsPanel,
  },
  {
    kind: "storage",
    name: "Storage",
    group: "Apps & Files",
    description: "Free space on your drives.",
    sizes: ["s", "m"],
    defaultSize: "m",
    panel: { width: 320, height: 260 },
    Tile: StorageTile,
    Panel: StoragePanel,
  },
  {
    kind: "recyclebin",
    name: "Recycle Bin",
    group: "Apps & Files",
    description: "What's in the Recycle Bin; open or empty it.",
    sizes: ["s", "m"],
    defaultSize: "s",
    panel: { width: 280, height: 220 },
    Tile: RecycleBinTile,
    Panel: RecycleBinPanel,
  },
  {
    kind: "mic",
    name: "Microphone",
    group: "Media",
    description: "Mute your microphone in one click, or set its level.",
    sizes: ["s", "m"],
    defaultSize: "s",
    panel: { width: 300, height: 220 },
    Tile: MicTile,
    Panel: MicPanel,
  },
  {
    kind: "theme",
    name: "Light or dark",
    group: "System",
    description: "Switch Windows between light and dark mode in one click.",
    sizes: ["s", "m"],
    defaultSize: "s",
    panel: { width: 300, height: 220 },
    Tile: ThemeTile,
    Panel: ThemePanel,
  },
  {
    kind: "showdesktop",
    name: "Show desktop",
    group: "System",
    description: "Minimise every window, then bring them back.",
    sizes: ["s", "m"],
    defaultSize: "s",
    panel: { width: 280, height: 140 },
    Tile: ShowDesktopTile,
    Panel: ShowDesktopPanel,
  },
  {
    kind: "progress",
    name: "Progress",
    group: "Productivity",
    description: "How much of the day, month and year has gone by.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 320, height: 280 },
    Tile: ProgressTile,
    Panel: ProgressPanel,
  },
  {
    kind: "speakers",
    name: "Speakers",
    group: "Media",
    description: "Your sound output with a volume slider.",
    sizes: ["m", "l"],
    defaultSize: "l",
    panel: { width: 300, height: 220 },
    Tile: SpeakersTile,
    Panel: SpeakersPanel,
  },
  {
    kind: "nowplaying",
    name: "Now playing",
    group: "Media",
    description: "Album art, track and play/pause for whatever is playing.",
    sizes: ["s", "m", "l"],
    defaultSize: "s",
    panel: { width: 260, height: 330 },
    Tile: NowPlayingTile,
    Panel: NowPlayingPanel,
  },
  {
    kind: "wallpaper",
    name: "Wallpaper",
    group: "Media",
    description: "Your desktop picture; click to change it.",
    sizes: ["s", "m"],
    defaultSize: "s",
    panel: { width: 340, height: 300 },
    Tile: WallpaperTile,
    Panel: WallpaperPanel,
  },
  {
    kind: "screentime",
    name: "Screen time",
    group: "Productivity",
    description: "Today's total, hour by hour and per app. Kept on this PC.",
    sizes: ["m", "l"],
    defaultSize: "l",
    panel: { width: 340, height: 460 },
    Tile: ScreenTimeTile,
    Panel: ScreenTimePanel,
  },
  {
    kind: "claude",
    name: "Claude",
    group: "AI",
    description: "Your Claude plan's Session and Weekly limits, from your Claude Code sign-in.",
    sizes: ["s", "m", "l"],
    defaultSize: "l",
    panel: { width: 340, height: 330 },
    ...aiWidget("claude_code"),
  },
  {
    kind: "claude_api",
    name: "Claude API",
    group: "AI",
    description: "Claude API spend this month, with an Admin API key.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 340, height: 360 },
    ...aiWidget("anthropic"),
  },
  {
    kind: "openai",
    name: "OpenAI",
    group: "AI",
    description: "OpenAI API spend this month, with an admin key.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 340, height: 360 },
    ...aiWidget("openai"),
  },
  {
    kind: "deepseek",
    name: "DeepSeek",
    group: "AI",
    description: "Your DeepSeek balance.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 340, height: 360 },
    ...aiWidget("deepseek"),
  },
  {
    kind: "chatgpt",
    name: "ChatGPT",
    group: "AI",
    description: "Your ChatGPT plan's Session and Weekly limits, from Codex CLI on this PC.",
    sizes: ["s", "m", "l"],
    defaultSize: "l",
    panel: { width: 340, height: 360 },
    ...aiWidget("codex"),
  },
  {
    kind: "openrouter",
    name: "OpenRouter",
    group: "AI",
    description: "Credits used and left on an OpenRouter key.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 340, height: 420 },
    ...aiWidget("openrouter"),
  },
  {
    kind: "kimi",
    name: "Kimi",
    group: "AI",
    description: "Your Moonshot AI (Kimi) balance.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 340, height: 380 },
    ...aiWidget("kimi"),
  },
  {
    kind: "ai",
    name: "AI usage",
    group: "AI",
    description: "Choose a service in its settings.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 350, height: 460 },
    Tile: AiTile,
    Panel: AiPanel,
    hidden: true,
  },
  {
    kind: "system",
    name: "System monitor",
    group: "System",
    description: "CPU, memory and network, with live charts.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 360, height: 400 },
    Tile: SystemTile,
    Panel: SystemPanel,
  },
  {
    kind: "weather",
    name: "Weather",
    group: "Weather",
    description: "Now and the next five days, from Open-Meteo.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 340, height: 250 },
    Tile: WeatherTile,
    Panel: WeatherPanel,
  },
  {
    kind: "todo",
    name: "To-do",
    group: "Productivity",
    description: "A quick task list, kept on this PC.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 330, height: 400 },
    Tile: TodoTile,
    Panel: TodoPanel,
  },
  {
    kind: "notes",
    name: "Notes",
    group: "Productivity",
    description: "A scratch pad that saves as you type.",
    sizes: ["s", "m", "l"],
    defaultSize: "s",
    panel: { width: 360, height: 340 },
    Tile: NotesTile,
    Panel: NotesPanel,
  },
  {
    kind: "pomodoro",
    name: "Pomodoro",
    group: "Productivity",
    description: "Focus and break timer with a chime.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 290, height: 270 },
    Tile: PomodoroTile,
    Panel: PomodoroPanel,
  },
  {
    kind: "worldclock",
    name: "World clock",
    group: "Clocks",
    description: "Times in other cities.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 330, height: 380 },
    Tile: WorldClockTile,
    Panel: WorldClockPanel,
  },
];



type Size = WidgetSize;
const ALL: Size[] = ["s", "m", "l"];
const SM: Size[] = ["s", "m"];
const ML: Size[] = ["m", "l"];

/** Compact registry entries for the widgets added in bulk. */
function w(
  kind: string,
  name: string,
  group: Group,
  description: string,
  sizes: Size[],
  defaultSize: Size,
  panel: [number, number],
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  parts: { Tile: ComponentType<WidgetProps<any>>; Panel: ComponentType<WidgetProps<any>> },
): WidgetKind {
  return { kind, name, group, description, sizes, defaultSize, panel: { width: panel[0], height: panel[1] }, ...parts };
}

const svc = (kind: string, name: string, group: Group, description: string, sizes = ALL, defaultSize: Size = "m", height = 400) =>
  w(kind, name, group, description, sizes, defaultSize, [340, height], serviceWidget(DEFS[kind]));

const MORE: WidgetKind[] = [
  // AI
  w("ai_limits", "AI limits", "AI", "Two AI plans' session limits side by side: Claude, ChatGPT or Gemini.", ML, "l", [340, 380], { Tile: AiLimitsTile, Panel: AiLimitsPanel }),
  svc("gemini", "Gemini", "AI", "Gemini CLI quota for Pro and Flash, from Gemini CLI's sign-in on this PC.", ALL, "l", 330),
  svc("glm", "GLM", "AI", "Your Z.ai GLM Coding Plan limits.", ALL, "l", 380),
  svc("opencode", "OpenCode", "AI", "Tokens in your latest OpenCode session, from its history on this PC.", ALL, "m", 360),
  svc("opencode_week", "OpenCode usage", "AI", "OpenCode tokens over the last 7 days, and your top model.", ALL, "m", 420),

  // Productivity
  w("timer", "Timer", "Productivity", "A one-click countdown with a chime.", ALL, "m", [300, 340], { Tile: T.TimerTile, Panel: T.TimerPanel }),
  w("stopwatch", "Stopwatch", "Productivity", "Start, pause and lap.", ALL, "m", [300, 360], { Tile: T.StopwatchTile, Panel: T.StopwatchPanel }),
  w("timeline", "Day timeline", "Productivity", "Your day on one line: PC use per hour, now, and daylight.", ML, "l", [360, 360], { Tile: T.DayTimelineTile, Panel: T.DayTimelinePanel }),
  svc("calendar", "Calendar", "Productivity", "Your next event, from a Google, Outlook or iCloud calendar's private address.", ALL, "l", 440),
  svc("currency", "Currency", "Productivity", "Convert between currencies at today's rate.", ALL, "m", 460),

  // Clocks
  w("clock", "Clock", "Clocks", "The time, large, with seconds and the date.", ALL, "m", [320, 330], { Tile: C.ClockTile, Panel: C.ClockPanel }),
  w("flipclock", "Flip clock", "Clocks", "Split-flap digits that flip every minute.", ALL, "m", [320, 330], { Tile: C.FlipClockTile, Panel: C.ClockPanel }),
  w("segclock", "Segment clock", "Clocks", "Seven-segment time with the weekday and date.", ALL, "m", [320, 330], { Tile: C.SegmentClockTile, Panel: C.ClockPanel }),
  w("analog", "Analog", "Clocks", "Hands and hour ticks on a small face.", ALL, "s", [260, 230], { Tile: C.AnalogMinimalTile, Panel: C.AnalogPanel }),
  w("worldanalog", "World clocks", "Clocks", "Analog faces for other cities.", ALL, "l", [330, 380], { Tile: C.WorldAnalogTile, Panel: C.WorldAnalogPanel }),
  w("worldtime", "World time", "Clocks", "One city in large digits with its UTC offset.", ALL, "m", [330, 380], { Tile: C.WorldTimeTile, Panel: C.WorldTimePanel }),
  w("date", "Date", "Clocks", "The full date.", ALL, "m", [300, 300], { Tile: C.DateTile, Panel: C.DatePanel }),
  w("daynumber", "Day number", "Clocks", "Today as a calendar page.", ALL, "s", [300, 300], { Tile: C.DayNumberTile, Panel: C.DatePanel }),
  w("today", "Today", "Clocks", "Weekday, date, time and the weather in one tile.", ALL, "l", [320, 360], { Tile: C.TodayTile, Panel: C.TodayPanel }),
  w("clockweather", "Clock & weather", "Clocks", "Time, date and temperature as three blocks.", ALL, "l", [320, 360], { Tile: C.ClockWeatherTile, Panel: C.ClockWeatherPanel }),
  w("sevendays", "7 days", "Clocks", "This week as a strip of days, with today's temperature.", ALL, "l", [340, 360], { Tile: C.SevenDaysTile, Panel: C.SevenDaysPanel }),

  // Weather
  w("wx_details", "Weather details", "Weather", "Wind, chance of rain, and today's range.", ALL, "m", [340, 420], { Tile: WeatherDetailsTile, Panel: WeatherPanel }),
  w("wx_forecast", "Today & forecast", "Weather", "Now, plus the next three days.", ALL, "l", [340, 420], { Tile: TodayForecastTile, Panel: WeatherPanel }),
  w("wx_status", "Weather status", "Weather", "The conditions in words.", ALL, "m", [340, 420], { Tile: WeatherStatusTile, Panel: WeatherPanel }),
  w("wx_hourly", "Hourly", "Weather", "The next few hours.", ALL, "l", [340, 420], { Tile: HourlyTile, Panel: WeatherPanel }),
  w("wx_sun", "Sunrise & sunset", "Weather", "Where the sun is between sunrise and sunset.", ALL, "m", [340, 420], { Tile: SunTile, Panel: WeatherPanel }),

  // System
  w("cpu", "CPU", "System", "Processor load as a ring.", ALL, "s", [360, 400], { Tile: P.CpuTile, Panel: SystemPanel }),
  w("ram", "RAM", "System", "Memory in use as a ring.", ALL, "s", [360, 400], { Tile: P.RamTile, Panel: SystemPanel }),
  w("network", "Network", "System", "Download and upload speed with a live line.", ALL, "m", [340, 230], { Tile: P.NetworkTile, Panel: P.NetworkPanel }),
  w("topcpu", "Top CPU", "System", "The apps using the most CPU.", ALL, "m", [320, 380], { Tile: P.TopCpuTile, Panel: TopAppsPanel }),
  w("topram", "Top RAM", "System", "The apps using the most memory.", ALL, "m", [320, 380], { Tile: P.TopRamTile, Panel: TopAppsPanel }),
  w("temperature", "Temperature", "System", "How warm the PC is, with CPU and memory load.", ALL, "m", [320, 320], { Tile: P.TemperatureTile, Panel: P.TemperaturePanel }),
  w("brightness", "Brightness", "System", "The screen's brightness, with a slider.", ALL, "m", [300, 220], { Tile: P.BrightnessTile, Panel: P.BrightnessPanel }),
  w("language", "Language", "System", "Your keyboard layout; click to switch.", SM, "s", [300, 260], { Tile: P.LanguageTile, Panel: P.LanguagePanel }),
  w("notifications", "Notifications", "System", "Open the notification center.", SM, "s", [300, 180], { Tile: P.NotificationsTile, Panel: P.NotificationsPanel }),
  w("keyboard", "Keyboard", "System", "Open the on-screen keyboard.", SM, "s", [300, 180], { Tile: P.KeyboardTile, Panel: P.KeyboardPanel }),
  w("battery_ring", "Battery ring", "System", "Charge as a ring that pulses while plugged in.", ALL, "s", [300, 220], { Tile: P.BatteryRingTile, Panel: BatteryPanel }),
  w("battery_core", "Battery core", "System", "An energy core with twelve cells lit to the charge.", ALL, "s", [300, 220], { Tile: P.BatteryCoreTile, Panel: BatteryPanel }),
  w("battery_dots", "Battery dots", "System", "A dot-matrix battery.", ALL, "m", [300, 220], { Tile: P.BatteryDotsTile, Panel: BatteryPanel }),
  w("battery_gauge", "Battery gauge", "System", "A fuel gauge for the battery.", ALL, "s", [300, 220], { Tile: P.BatteryGaugeTile, Panel: BatteryPanel }),
  w("dayprogress", "Day progress", "System", "How much of today has passed.", ALL, "s", [320, 280], { Tile: P.DayProgressTile, Panel: ProgressPanel }),
  w("monthprogress", "Month progress", "System", "How much of the month has passed.", ALL, "s", [320, 280], { Tile: P.MonthProgressTile, Panel: ProgressPanel }),
  w("yearprogress", "Year progress", "System", "The year as a filling grid of dots.", ALL, "m", [380, 330], { Tile: P.YearProgressTile, Panel: P.YearProgressPanel }),

  // Media
  w("now_compact", "Now playing (compact)", "Media", "Title, artist and play/pause.", ML, "m", [260, 330], { Tile: P.NowCompactTile, Panel: NowPlayingPanel }),
  w("now_mini", "Now playing mini", "Media", "A round play button with the cover.", ALL, "s", [260, 330], { Tile: P.NowMiniTile, Panel: NowPlayingPanel }),
  w("now_bar", "Now playing bar", "Media", "Track, a bar, and skip.", ML, "l", [260, 330], { Tile: P.NowBarTile, Panel: NowPlayingPanel }),

  // Apps & Files
  w("apps", "Apps", "Apps & Files", "Start, your apps and open windows.", SM, "s", [340, 440], { Tile: P.AppsTile, Panel: P.AppsPanel }),
  w("recent", "Recently opened", "Apps & Files", "Files and folders you opened lately.", ALL, "m", [340, 420], { Tile: P.RecentTile, Panel: P.RecentPanel }),
  w("search", "Search", "Apps & Files", "Search the web, or open a folder by its path.", ALL, "m", [340, 220], { Tile: P.SearchTile, Panel: P.SearchPanel }),
  w("commands", "Commands", "Apps & Files", "Open apps or actions by typing a word.", ALL, "s", [340, 400], { Tile: P.CommandsTile, Panel: P.CommandsPanel }),
  w("screenshot", "Screenshot", "Apps & Files", "Capture the screen, a window or a region.", SM, "s", [320, 220], { Tile: P.ScreenshotTile, Panel: P.ScreenshotPanel }),

  // Online & Mail
  w("gallery", "Wallpaper gallery", "Online & Mail", "Browse this week's pictures and set one as your background.", SM, "s", [380, 330], { Tile: P.GalleryTile, Panel: P.GalleryPanel }),
  svc("mail", "Mail", "Online & Mail", "Unread mail in Gmail, iCloud or any IMAP inbox, with an app password.", ALL, "m", 460),
  svc("github_activity", "GitHub activity", "Online & Mail", "Your contribution grid and streak.", ALL, "l", 380),
  svc("github_notifications", "GitHub", "Online & Mail", "Unread GitHub notifications.", ALL, "s", 420),
  svc("github_overview", "GitHub overview", "Online & Mail", "Notifications, open pull requests and stars.", ALL, "m", 460),
  svc("leetcode", "LeetCode", "Online & Mail", "Problems solved and your streak.", ALL, "m", 400),

  // Finance
  w("revenue_total", "Total revenue", "Finance", "All your stores' last 30 days together, with a monthly goal.", ALL, "m", [340, 420], { Tile: TotalRevenueTile, Panel: TotalRevenuePanel }),
  svc("stripe", "Stripe", "Finance", "Net revenue, last 30 days.", ALL, "m", 520),
  svc("stripe_chart", "Stripe chart", "Finance", "Stripe revenue as a line.", ML, "l", 520),
  svc("polar", "Polar", "Finance", "Net revenue, last 30 days.", ALL, "m", 520),
  svc("polar_chart", "Polar chart", "Finance", "Polar revenue as a line.", ML, "l", 520),
  svc("dodo", "Dodo Payments", "Finance", "Payments, last 30 days.", ALL, "m", 520),
  svc("dodo_chart", "Dodo chart", "Finance", "Dodo Payments revenue as a line.", ML, "l", 520),
  svc("shopify", "Shopify", "Finance", "Sales, last 30 days.", ALL, "m", 560),
  svc("shopify_chart", "Shopify chart", "Finance", "Shopify sales as a line.", ML, "l", 560),
  svc("lemonsqueezy", "Lemon Squeezy", "Finance", "Revenue after tax and refunds, 30 days.", ALL, "m", 520),
  svc("lemonsqueezy_chart", "Lemon Squeezy chart", "Finance", "Lemon Squeezy revenue as a line.", ML, "l", 520),
  svc("paddle", "Paddle", "Finance", "Your earnings, 30 days.", ALL, "m", 520),
  svc("paddle_chart", "Paddle chart", "Finance", "Paddle earnings as a line.", ML, "l", 520),
  svc("gumroad", "Gumroad", "Finance", "Earnings after Gumroad's fee, 30 days.", ALL, "m", 520),
  svc("gumroad_chart", "Gumroad chart", "Finance", "Gumroad earnings as a line.", ML, "l", 520),

  // Stocks
  svc("stock", "Stock", "Stocks", "One ticker with today's line.", ALL, "l", 420),
  svc("watchlist", "Watchlist", "Stocks", "Up to six tickers at a glance.", ALL, "l", 440),
  svc("crypto", "Crypto", "Stocks", "A coin's price, 24-hour change and line.", ALL, "l", 440),

  // Analytics
  svc("plausible", "Plausible", "Analytics", "Visitors this week and the trend.", ALL, "m", 520),
  svc("dub", "Dub", "Analytics", "Clicks on your Dub links this week.", ALL, "m", 440),

  // Developer
  svc("resend", "Resend", "Developer", "Emails sent and delivered this week.", ALL, "m", 480),
  svc("linear", "Linear", "Developer", "Issues assigned to you, and what's in progress.", ALL, "m", 480),
  svc("supabase", "Supabase", "Developer", "API requests and project health.", ALL, "m", 520),

  // Social
  svc("x", "X", "Social", "Followers on your X account.", ALL, "s", 400),
  svc("youtube", "YouTube", "Social", "Channel subscribers.", ALL, "s", 400),
  svc("instagram", "Instagram", "Social", "Followers on a professional account.", ALL, "s", 360),
];

WIDGETS.push(...MORE);

export const byKind = (kind: string) => WIDGETS.find((w) => w.kind === kind);

export const SIZE_LABEL: Record<WidgetSize, string> = { s: "Small", m: "Medium", l: "Large" };
