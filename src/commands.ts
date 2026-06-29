import { invoke } from '@tauri-apps/api/core';

export interface SessionView {
  id: number;
  rows: number;
  cols: number;
}

export interface TerminalOutputEvent {
  sessionId: number;
  bytes: number[];
}

export function createSession(program?: string, args?: string[]): Promise<SessionView> {
  return invoke<SessionView>('create_session', { program, args });
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

export function readSession(sessionId: number): Promise<TerminalOutputEvent> {
  return invoke<TerminalOutputEvent>('read_session', { sessionId });
}
