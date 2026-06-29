# `baton` — 최대 속도 터미널 코어

## 0. 목표 / 비목표 / 측정 기준

**목표**: webview 에이전트 터미널 중 렌더·스트리밍 속도 최상위 티어. 에이전트가 로그 폭포수를 뱉어도 UI 안 끊김.

**측정 가능한 타깃** (이걸 못 맞추면 실패):

| 항목 | 타깃 |
|---|---|
| PTY throughput 흡수 | ≥100MB/s 출력에도 프레임 드랍 없음 (coalesced) |
| vtebench `cat`/`scroll` | Alacritty 동급 ±20% |
| 입력 지연 (keypress→glyph) | PTY 왕복 위에 +8ms 이내, 1프레임 내 |
| idle CPU | ~0% (이벤트 드리븐, 폴링 금지) |
| 메모리 | scrollback N줄 초과분은 spill, 상한 고정 |
| 콜드 스타트 | <300ms |

**비목표**: 풀 IDE, 풀 SCM GUI. 핫패스 밖이라 후반에 가볍게만.

---

## 1. 단 하나의 핵심 결정

> **VT 파싱과 스크린 그리드는 Rust가 소유한다. webview는 "chrome"(탭·diff·세션리스트)만 그린다.
> 터미널 셀 영역은 네이티브 GPU 서피스로 직접 렌더한다.**

이게 속도의 90%다. 이유:

- xterm.js에 raw PTY 바이트를 넘기면 **JS가 VT 파싱 + 그리드 보유 + 캔버스 드로우**를 다 함 → webview가 병목. mac WebKit / Linux WebKitGTK에서 일관성·속도 모두 손해.
- Rust에서 파싱하고 그리드를 들고 있으면, OS별 webview 성능 편차를 **터미널 핫패스에서 완전히 우회**한다.

나중에 바꾸기 어려운 결정이라 처음에 박는다. (escape hatch는 §11.)

---

## 2. 레이어 구조

```text
┌───────────────────────────────────────────────────────────┐
│  Webview (Tauri, 시스템 webview) — "CHROME ONLY"            │
│  탭/분할트리, diff/blame, 세션리스트, 태스크보드, 설정       │
│  - 터미널 셀은 안 그림. 셀 영역은 투명 hole로 비워둠         │
└──────────────┬────────────────────────────────────────────┘
       control plane (JSON IPC: resize/create/focus/scroll)
┌──────────────▼────────────────────────────────────────────┐
│  Rust core (tokio)                                          │
│                                                             │
│  ┌─────────────┐  bytes  ┌──────────────┐  damage          │
│  │ PTY reader  ├────────►│ VT parser +  ├────────┐         │
│  │ (1 thread/  │ ring buf│ Grid (Term)  │        │         │
│  │  session)   │◄────────┤ alacritty_   │        ▼         │
│  └─────┬───────┘ input   │ terminal     │  ┌──────────────┐│
│        │ write           └──────────────┘  │ GPU renderer ││
│        │ keypress                          │ wgpu + glyph ││
│                                            │ atlas, damage││
│                                            │ → 네이티브   ││
│                                            │   child layer││
│                                            └──────────────┘│
│  ┌────────────────────────────────────────────────────┐    │
│  │ Orchestration (격리, async) — 핫패스 안 건드림        │    │
│  │ 로컬 MCP · OpenAI호환 게이트웨이 · conductor/worker   │    │
│  └────────────────────────────────────────────────────┘    │
│  SQLite (세션/스크롤백 spill/스페이스 영속, WAL)             │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Hot path: PTY → Grid → GPU (가장 중요)

1. **PTY**: `portable-pty`(wezterm 크레이트)로 세션당 PTY 1개. 읽기는 **블로킹 스레드 1개/세션**(tokio `spawn_blocking`), UI 스레드 절대 안 막음.
2. **Coalescing(핵심)**: 읽은 바이트를 세션별 ring buffer에 적재. 바이트당 깨우지 말고 **프레임 캐던스(≈8–16ms)로 배치 flush** 또는 idle 시 flush. 에이전트가 `yes` 갈겨도 파서는 묶음으로 소비.
3. **Parse + Grid**: `alacritty_terminal`의 `Term`/`Grid`로 VT 시퀀스 파싱 + 그리드 갱신. **damage tracking**으로 바뀐 셀/라인만 마킹.
4. **Render**: `wgpu`로 글리프 아틀라스 GPU 렌더. **damage 영역만 재드로우**. 표시 스크롤백만 GPU에 올림.
5. **Backpressure**: ring buffer 상한 도달 시 PTY 읽기 일시 정지(OS 파이프 버퍼가 producer를 자연 제동). 드랍 대신 제동 — 출력 무결성 유지.

> 핵심 격언: **producer 속도(에이전트 출력)와 render 속도(60–120fps)를 디커플.**
> 파서는 producer 페이스로, 렌더는 frame 페이스로.

---

## 4. IPC: control plane vs data plane 분리

- **Data plane** (PTY 셀 데이터): webview를 **거치지 않음**. Rust 그리드 → 네이티브 GPU 서피스 직결. JSON 직렬화 0.
- **Control plane** (resize, create/kill, focus, scroll offset, selection): Tauri v2 command/`Channel`로 JSON. 저빈도라 비용 무시.
- webview ↔ GPU 서피스 합성: `raw-window-handle`로 webview 위/아래 네이티브 child layer 합성, 터미널 영역만 webview에서 투명 hole.

> ⚠️ **최대 리스크 지점**: webview와 네이티브 GPU 서피스 합성은 OS별로 까다로움(macOS `NSView` 레이어, Windows child HWND/DComp). 이 아키텍처의 가장 어려운 1%. §11에서 단계적으로.

---

## 5. Scrollback / 메모리

- 인메모리 ring: 표시 + 최근 N줄(예 10k)만.
- 초과분: **mmap 파일 또는 SQLite spill**, 스크롤 시 페이지인.
- GPU엔 viewport에 보이는 줄만 업로드. 글리프 아틀라스 LRU.

---

## 6. 입력 지연 경로

- keypress → (control plane 우회) → 세션 PTY write에 **최단 경로**. 무거운 레이어 통과 금지.
- 옵션: predictive/local echo (네트워크 SSH 세션에서 체감 지연 ↓). MVP엔 빼고 측정 후 결정.
  `// ponytail: 로컬 PTY는 빠르니 predictive echo는 SSH 붙일 때만`

---

## 7. Orchestration 레이어 (격리가 규칙)

멀티에이전트 오케스트레이션을 지원하되 **핫패스와 물리적으로 분리**:

- conductor/worker: commander(claude)가 worker(codex 등) 지휘. `open_session`/`send`/`report`.
- 로컬 MCP 엔드포인트 + OpenAI 호환 게이트웨이 — 전부 **async HTTP, 별 task**. 오케스트레이션 부하가 렌더를 절대 못 막게.
- 에이전트도 결국 PTY 세션 1개 → 동일 hot path 재사용. 오케스트레이션은 "어느 세션에 뭘 send/report"만 관장.

---

## 8. 핫패스 밖 기능 (간단히)

webview에서 평범하게 web UI로:

- 에디터(외부변경 reload), in-app 브라우저(네이티브 child webview), SCM(log graph/diff/blame), ripgrep 풀텍스트 검색(스트리밍), 분할트리/탭/스페이스 영속.
- 전부 control plane 쪽. 속도 spec 대상 아님.

---

## 9. 기술 스택 확정

| 레이어 | 선택 | 이유 |
|---|---|---|
| 셸/창/chrome UI | **Tauri v2** | 가벼운 바이너리·메모리·시작 |
| PTY | **portable-pty** | 크로스플랫폼, wezterm 검증 |
| VT 파서+그리드 | **alacritty_terminal** | 속도·damage tracking 검증된 코어 |
| GPU 렌더 | **wgpu** + 글리프 아틀라스 | 크로스플랫폼 GPU, WebKitGTK 우회 |
| 폰트 | crossfont / cosmic-text | 셰이핑·아틀라스 |
| async | **tokio** | PTY 스레드풀 + 오케스트레이션 |
| 영속 | **SQLite (WAL)** + mmap spill | 세션/스크롤백/스페이스 |
| 오케스트레이션 | TS(webview측) → MCP/게이트웨이 HTTP | 핫패스 분리 |

---

## 10. 성능 검증 (spec의 일부, 협상 불가)

- **vtebench** 회귀: `cat`, `scrolling`, `unicode` 케이스 CI에 고정.
- 인공 폭포수(`yes`, 100MB 로그 `cat`) 흡수 + 프레임 드랍 카운터.
- 입력 지연: keypress→glyph 프레임 타임스탬프 측정.
- idle CPU 0% assert (폴링 회귀 탐지).

---

## 11. 빌드 단계 (얇은 슬라이스부터)

1. **슬라이스 1 — 데이터 플레인 규율 먼저**: Tauri + portable-pty + `alacritty_terminal` 파싱, **렌더는 xterm.js + WebGL addon**(webview)으로 시작. Channel로 coalesced 바이트 스트림. → 빨리 동작하는 터미널 확보. *이게 escape hatch이자 MVP.*
2. **측정**: vtebench·폭포수. 슬라이스1이 타깃 맞으면 **거기서 멈춰도 됨**.
3. **슬라이스 2 — 못 맞추면 그때 §1 풀버전**: 그리드 소유권을 Rust로 옮기고 wgpu 네이티브 서피스 합성으로 교체. (가장 어려운 §4 합성 리스크는 여기서만 감수)
4. 오케스트레이션·에디터·브라우저는 슬라이스 1 이후 병렬, 핫패스 무관.

> **컷라인**: 슬라이스 1로 타깃을 맞추면 네이티브 GPU 합성(슬라이스 2)은 **짓지 마라**. "최대 속도"가 목표여도, 측정이 "이미 충분"이라 말하면 그게 정답. xterm.js+WebGL의 천장은 "Linux WebKitGTK에서 vtebench 미달"일 때만 실제로 보인다 → 그 순간에만 슬라이스 2.

---

## 12. 리스크 요약

| 리스크 | 영향 | 완화 |
|---|---|---|
| webview↔GPU 서피스 합성(§4) | 높음 | 슬라이스 2로 격리, 슬라이스 1로 우회 가능 |
| 그리드 소유권 Rust 이전 = 큰 리라이트 | 중 | 처음부터 Rust 파싱(슬라이스1)이라 렌더만 교체 |
| Tauri webview OS 편차 | 중 | chrome만 webview, 셀은 네이티브라 영향 최소 |

---

## 한 줄 요지

속도는 *"VT 파싱·그리드를 Rust가 갖고, 셀은 네이티브 GPU로, PTY는 coalescing+backpressure로, webview는 chrome만"*에서 나온다. 단 그 풀버전(네이티브 합성)은 어려우니 **xterm.js+WebGL로 먼저 짜서 측정하고, 미달일 때만 네이티브로 내려가라.**
