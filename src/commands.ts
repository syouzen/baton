import { Channel, invoke } from '@tauri-apps/api/core';

export interface SessionView {
  id: number;
  rows: number;
  cols: number;
}

export type TerminalOutputHandler = (bytes: ArrayBuffer) => void;

export interface TerminalSnapshotView {
  id: number;
  rows: number;
  cols: number;
  lines: string[];
}

export interface Slice1MeasurementReport {
  bytesRead: number;
  framesRead: number;
  maxFrameBytes: number;
  elapsedMs: number;
  throughputMibPerSecond: number;
  inputRoundTripMs: number;
  resizeLatencyMicros: number;
}

export function createSession(
  program?: string,
  args?: string[],
  onOutput?: TerminalOutputHandler,
): Promise<SessionView> {
  const output = new Channel<ArrayBuffer>();
  if (onOutput) {
    output.onmessage = onOutput;
  }
  return invoke<SessionView>('create_session', { program, args, output });
}

export function writeSession(sessionId: number, bytes: number[]): Promise<void> {
  return invoke<void>('write_session', { sessionId, bytes });
}

export function resizeSession(
  sessionId: number,
  rows: number,
  cols: number,
  pixelWidth?: number,
  pixelHeight?: number,
): Promise<SessionView> {
  return invoke<SessionView>('resize_session', {
    sessionId,
    rows,
    cols,
    pixelWidth,
    pixelHeight,
  });
}

export function killSession(sessionId: number): Promise<void> {
  return invoke<void>('kill_session', { sessionId });
}

export function listSessions(): Promise<SessionView[]> {
  return invoke<SessionView[]>('list_sessions');
}

export function readSession(sessionId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>('read_session', { sessionId });
}

export function snapshotSession(sessionId: number): Promise<TerminalSnapshotView> {
  return invoke<TerminalSnapshotView>('snapshot_session', { sessionId });
}

export function runBaselineMeasurement(): Promise<Slice1MeasurementReport> {
  return invoke<Slice1MeasurementReport>('run_baseline_measurement');
}
