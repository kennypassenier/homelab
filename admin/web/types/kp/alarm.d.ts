// The kp-themes module chassis-rs serves at /static/kp/js/alarm.js (kp-themes
// 8.0.0); the part the refusal page uses, copied from kp-themes js/alarm.d.ts.
export type AlarmReason = "ack" | "timeout" | "escape";
export type AlarmOptions = {
  title: string;
  detail?: string;
  code?: string;
  mode?: "ack" | "auto";
  seconds?: number;
  escape?: boolean;
  action?: string;
};
export function showAlarm(options: AlarmOptions): Promise<AlarmReason>;
