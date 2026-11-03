/* Proposed interaction fixtures. No connection, generation, credentials, or approvals are executed. */
(() => {
  'use strict';

  window.AgentStudioData = {
    version: 1,
    title: 'Agent interaction studio',
    description: 'A local, interactive proposal based on your survey. Try the controls and failure paths before production implementation.',
    catalogNote: 'Sample catalog for this prototype. Model availability, effort controls, speed controls, and mid-task changes are illustrative; production must use each provider’s advertised capabilities.',
    surveyNote: 'Selections identify preferred examples. Unselected rows remain undecided; they are not rejected features. Original choices and comments are preserved in survey-preferences.json.',
    referenceLabels: {
      chat: 'Chat train',
      artifact: 'Chat opened from an existing item',
      image: 'Image generator',
      text: 'Text generator',
      builder: 'Dashboard generator',
      free: 'Free agent',
      menu: 'Right-click menus'
    },
    modalities: [
      {
        id: 'chat', label: 'Chat',
        description: 'A continuing conversation with the familiar window, message pairs, or individual message cards.',
        defaultPrompt: 'Help me organize the research for this project.',
        interactive: true, defaultPresentation: 'pairs'
      },
      {
        id: 'source-chat', label: 'Chat from an item',
        description: 'Open chat from an existing image, document, or dashboard, with its reference and model choice visible before the first send.',
        defaultPrompt: 'Use this dashboard as the starting point and explain what we could improve.',
        interactive: true, defaultPresentation: 'pairs'
      },
      {
        id: 'image', label: 'Image generator',
        description: 'An attached image editor and output gallery. Continuing dialogue is an optional mode, off by default.',
        defaultPrompt: 'Create a warm architectural study of a small reading room with soft daylight.',
        interactive: false, defaultPresentation: 'pairs'
      },
      {
        id: 'text', label: 'Text generator',
        description: 'Generate writing into a linked document or note using explicit references and the shared model controls.',
        defaultPrompt: 'Write a concise project brief from the connected research notes.',
        interactive: false, defaultPresentation: 'window'
      },
      {
        id: 'dashboard', label: 'Dashboard generator',
        description: 'Create and revise a dashboard, browse earlier outputs, and send additional instructions during work.',
        defaultPrompt: 'Build a compact project dashboard with milestones, open questions, and the connected data.',
        interactive: true, defaultPresentation: 'window'
      },
      {
        id: 'free-agent', label: 'Free agent',
        description: 'A named board agent with a continuous conversation window, visible work updates, and distinct Pause, Resume, and Stop actions.',
        defaultPrompt: 'Review the connected project material and prepare a short list of next actions.',
        interactive: true, defaultPresentation: 'window'
      }
    ],
    providers: [
      {
        id: 'codex', label: 'Codex', kind: 'subscription', needsKey: false, sample: true,
        models: [
          { id: 'gpt-6.1-sol', label: 'Sol', efforts: ['Low', 'Medium', 'High', 'Extra high'], speeds: ['Standard', 'Fast'], canChangeEffortWhileRunning: true },
          { id: 'gpt-6-astra', label: 'Astra', efforts: ['Low', 'Medium', 'High', 'Extra high'], speeds: ['Standard'], canChangeEffortWhileRunning: false }
        ]
      },
      {
        id: 'cursor', label: 'Cursor', kind: 'subscription', needsKey: false, sample: true,
        models: [
          { id: 'sample-cursor-model', label: 'Sample model', efforts: ['Standard', 'Deep'], speeds: ['Standard'], canChangeEffortWhileRunning: false }
        ]
      },
      {
        id: 'openai', label: 'OpenAI API', kind: 'api', needsKey: true, sample: true,
        models: [
          { id: 'gpt-image-2', label: 'GPT Image 2', efforts: [], speeds: ['Standard'], canChangeEffortWhileRunning: false },
          { id: 'sample-api-text', label: 'Sample text model', efforts: ['Low', 'Medium', 'High'], speeds: ['Standard'], canChangeEffortWhileRunning: false }
        ]
      },
      {
        id: 'local', label: 'Local model', kind: 'local', needsKey: false, sample: true,
        models: [
          { id: 'sample-local-model', label: 'Sample local model', efforts: [], speeds: [], canChangeEffortWhileRunning: false }
        ]
      }
    ],
    scenarios: [
      {
        id: 'chat', label: 'Normal conversation', modality: 'chat', status: 'idle',
        description: 'Start a conversation, send a follow-up, and change how the same history is arranged.',
        expected: 'One window, message-pair cards, and individual message cards show the same conversation. Changing presentation retains the unsent draft and sends no new request.'
      },
      {
        id: 'source-chat', label: 'Chat from an existing item', modality: 'source-chat', status: 'idle',
        description: 'Open chat from a dashboard and choose the model before sending its first message.',
        expected: 'The provider and model control is immediately available. The source remains an explicit reference rather than a hidden prompt attachment.'
      },
      {
        id: 'missing-key', label: 'Add an API key', modality: 'image', status: 'idle',
        description: 'Select the API provider with no key saved, then open its separate chevron.',
        provider: 'openai', keyState: 'missing',
        expected: 'Clicking the provider row selects it. Its separate chevron opens Add API key. A masked credential field preserves the image prompt and does not submit a generation.'
      },
      {
        id: 'expired-key', label: 'Replace an expired key', modality: 'image', status: 'error',
        description: 'A saved key is refused. Replace it from the API provider’s chevron or the visible recovery action.',
        provider: 'openai', keyState: 'expired',
        expected: 'Change API key remains reachable even when a key is already saved. The next explicit request uses the replacement; the prompt, references, and prior images remain intact.'
      },
      {
        id: 'image-independent', label: 'Independent image requests', modality: 'image', status: 'idle',
        description: 'Generate a picture with interactive mode off and browse its variations in the gallery.',
        interactive: false,
        expected: 'Independent requests do not inherit earlier dialogue. The image editor, count, aspect, seed, and result actions remain available.'
      },
      {
        id: 'image-interactive', label: 'Continue an image dialogue', modality: 'image', status: 'idle',
        description: 'Enable interactive mode, create an image, then ask for a darker background without repeating the brief.',
        interactive: true,
        expected: 'Follow-ups use the same conversation path and chat-train presentation behavior. A fourth wire makes continuity explicit; cutting it resets only later context and keeps existing images.'
      },
      {
        id: 'dashboard-steering', label: 'Steer a running dashboard task', modality: 'dashboard', status: 'running',
        description: 'While the dashboard is being prepared, send an extra instruction and try changing effort.',
        expected: 'The interface reports whether the instruction reaches the task now or is queued. Effort can change during work only for a supported provider/model, with clear acknowledgement of when it takes effect.'
      },
      {
        id: 'quiet-provider', label: 'Waiting without invented updates', modality: 'dashboard', status: 'running',
        description: 'The provider remains connected but has not reported a new work event.',
        quiet: true,
        expected: 'Elapsed time and time since the last real event remain visible. Waiting for service is stated honestly; no synthetic activity is inserted to suggest progress.'
      },
      {
        id: 'approval-wait', label: 'A specific approval is required', modality: 'dashboard', status: 'approval',
        description: 'A task pauses at a clearly named action outside its effective grant.',
        expected: 'Show the action and target with Allow and Refuse. The global access control stays visible, and provider tool access remains distinct from permission to apply board changes.'
      },
      {
        id: 'failure', label: 'Recover from a failed request', modality: 'text', status: 'error',
        description: 'A model request fails after inputs have been prepared.',
        expected: 'The error explains the available next action. The draft and references remain present; Retry is explicit and does not silently replay a paid request.'
      },
      {
        id: 'continuity-cutoff', label: 'Cut the continuity wire', modality: 'dashboard', status: 'idle',
        description: 'Disconnect the fourth wire between earlier work and the next revision, then continue downstream.',
        expected: 'The downstream task starts without inherited context. Earlier outputs and the upstream conversation remain available; cutting a wire does not delete existing work or alter unrelated branches.'
      },
      {
        id: 'free-agent-pause', label: 'Pause and resume a free agent', modality: 'free-agent', status: 'running',
        description: 'Open the agent’s continuous conversation window and exercise Pause, Resume, and Stop.',
        expected: 'Pause visibly suspends the task, Resume continues it, and Stop ends it. The conversation window and unsent draft remain available.'
      }
    ],
    decisions: [
      {
        id: 'shared-picker', title: 'Use the dashboard model control across agent tools', source: 'p02–p09; p02 comment',
        detail: 'Keep provider, model, effort, and speed distinct within one consistent control. The selector must be present before the first send, including chat opened from an existing item.'
      },
      {
        id: 'model-memory', title: 'Remember model preferences across every mode', source: 'p10 selection and comment',
        detail: 'Extend the image generator’s remembered-model pattern to every modality. Production should retain each mode’s authored selection and expose the preference used for new tools.'
      },
      {
        id: 'running-effort', title: 'Allow supported effort changes during a task', source: 'p11 selection; p12 correction',
        detail: 'The user corrected the provider-switch row: effort, rather than provider, may change during work where supported. Show whether the change applies now or to the next turn. Do not silently switch the provider or model of an active task.'
      },
      {
        id: 'menu-language', title: 'Use compact right-click menu rows at every depth', source: 'm01–m09 preferences; m02 and m06 comments',
        detail: 'Use a single row style with short names, icons, checks, dividers, and consistent chevrons. The existing submenu and Settings arrangements were not endorsed; the studio proposes their replacement. Place explanations in hover help or beside the menu.'
      },
      {
        id: 'key-entry', title: 'Separate API selection from key management', source: 'Fixed requirement; s07 comment',
        detail: 'The provider row selects the API service. Its separate chevron opens Add API key or Change API key. A dedicated masked field handles credentials while keeping the composer’s prompt intact. Expiry recovery must remain reachable.'
      },
      {
        id: 'global-access', title: 'One persistent access preference', source: 'Fixed requirement; s02 comment; s01, s04–s06 selections',
        detail: 'Expose one visible setting shared across agent types and open application instances, remembered across restarts and days. Show effective access and any next-task boundary. Keep source restrictions and board-apply authority inspectable as distinct permissions.'
      },
      {
        id: 'chat-presentations', title: 'Reuse the actual chat-train conversation behavior', source: 'c01, c03–c08, c10–c11; c02 preference',
        detail: 'Support one conversation window, message pairs, and individual message cards over the same history. Preserve drafts, branch points, full-conversation access, names, and folds without resending history. Use the free agent’s window as the preferred continuous-window example.'
      },
      {
        id: 'interactive-image', title: 'Interactive image mode starts off', source: 'Fixed requirement; c12',
        detail: 'Independent image generation stays the default. When enabled, interactive mode adopts chat-train dialogue and presentation behavior while retaining the image editor and gallery.'
      },
      {
        id: 'continuity-wire', title: 'Make inherited context a dedicated fourth wire', source: 'c09 comment',
        detail: 'For image and dashboard tools, a dedicated fourth wire carries agent continuity. Cutting it resets inherited context only for downstream work. Existing images, dashboard revisions, upstream history, and unrelated branches remain intact.'
      },
      {
        id: 'steering', title: 'Let dashboard tasks receive follow-up instructions', source: 'i05–i07 preferences and i05 comment',
        detail: 'Use the free agent’s running-message and draft patterns. Dashboard users can send follow-ups while work is running, with an explicit acknowledgement that the message was delivered now or queued.'
      },
      {
        id: 'image-results', title: 'Keep the image gallery and attached editor', source: 'a01–a04, a08–a09; i03–i04',
        detail: 'Retain the preferred attached panel, selectable output history, image reference roles, image count, aspect, seed, and copy/save/place actions. Dialogue mode adds conversation rather than replacing image controls.'
      },
      {
        id: 'clear-generation-labels', title: 'Name when generation happens', source: 'a09 comment',
        detail: 'Replace Single and Live with Generate on request and Regenerate when inputs change. These choices govern generation triggers and remain distinct from interactive conversation.'
      },
      {
        id: 'honest-progress', title: 'Show actual work and honest waiting', source: 'r01–r09 selections; fixed progress requirement',
        detail: 'Reuse free-agent status, commentary, and tool start/finish events; image activity folds; and chat’s scrollback. Show elapsed time and last-event age during long work. Keep quiet-provider waits distinct from real progress.'
      },
      {
        id: 'pause-stop', title: 'Keep Pause, Resume, and Stop distinct', source: 'r02–r04 preferences',
        detail: 'Use the free agent’s lifecycle language and the image generator’s clear Play/Stop action where appropriate. Controls must report the task’s actual state rather than merely changing their labels.'
      },
      {
        id: 'review-changes', title: 'Prototype a clearer Apply and Discard review', source: 'a05 comment; s06 selection',
        detail: 'No existing review pattern was selected as satisfactory. This studio proposes a visible preview and explicit Apply/Discard actions; it does not treat provider access as automatic board acceptance.'
      },
      {
        id: 'undecided', title: 'Leave unselected survey rows open', source: 'Submitted survey interpretation',
        detail: 'No selection means no preferred current example was chosen. Preserve those rows and comments for later review instead of interpreting them as a request to remove the feature.'
      }
    ],
    help: {
      provider: 'The service or app connection that runs the agent.',
      model: 'The selected model within that service. This studio uses sample choices.',
      effort: 'The requested thinking effort. During-task changes are shown only for the sample model that supports them.',
      speed: 'A response-speed choice, separate from thinking effort. Availability comes from the service.',
      access: 'A global persistent preference; effective tool access and board-apply authority remain visible separately.',
      continuity: 'A dedicated fourth wire carries earlier conversation context into later image or dashboard work.',
      interactive: 'Continuing dialogue using the same behavior as chat train. Off by default for images.',
      generation: 'Generate on request or regenerate when inputs change. This does not select whether dialogue continues.',
      activity: 'Public commentary and observed tool events. A quiet provider may send no new events.',
      key: 'Prototype only: any entered value is a simulated credential and must not be a real API key.'
    },
    patterns: [
  {
    "id": "p01",
    "label": "Provider / harness choice",
    "preferred": [
      "artifact"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p02",
    "label": "Current provider / model label",
    "preferred": [
      "chat",
      "builder"
    ],
    "comment": "with the exeption of when the chat train has been spawned from an artifact",
    "undecided": false
  },
  {
    "id": "p03",
    "label": "Model selector before first send",
    "preferred": [
      "builder"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p04",
    "label": "Provider-catalog model list",
    "preferred": [
      "builder"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p05",
    "label": "Provider + model in one picker",
    "preferred": [
      "builder"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p06",
    "label": "Nested Model submenu",
    "preferred": [
      "builder"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p07",
    "label": "Effort level submenu",
    "preferred": [
      "builder"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p08",
    "label": "Speed submenu",
    "preferred": [
      "builder"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p09",
    "label": "Default / automatic model choice",
    "preferred": [
      "chat",
      "builder"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p10",
    "label": "Remember model for new tools",
    "preferred": [
      "image"
    ],
    "comment": "this memory should be a feture on all modes",
    "undecided": false
  },
  {
    "id": "p11",
    "label": "Freeze choices for the running request",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "p12",
    "label": "Switch provider after / during task",
    "preferred": [
      "free"
    ],
    "comment": "not providor but user should be able to alter efort during a task where apliable",
    "undecided": false
  },
  {
    "id": "m01",
    "label": "Compact icon + action rows",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "m02",
    "label": "Consistent submenu chevrons",
    "preferred": [],
    "comment": "none of them do this well sub menues are a mess",
    "undecided": true
  },
  {
    "id": "m03",
    "label": "Checked current selection",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "m04",
    "label": "Regular grouping / inset separators",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "m05",
    "label": "Overflow menu for secondary actions",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "m06",
    "label": "Settings beside the model control",
    "preferred": [],
    "comment": "none do this well",
    "undecided": true
  },
  {
    "id": "m07",
    "label": "Same menu skin at every depth",
    "preferred": [
      "menu"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "m08",
    "label": "Short labels; no explanatory prose",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "m09",
    "label": "Menu owns scrolling / wheel input",
    "preferred": [
      "menu",
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c01",
    "label": "Persistent multi-turn dialogue",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c02",
    "label": "Everything in one chat window",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c03",
    "label": "Train of message-pair cards",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c04",
    "label": "Full train: one message per card",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c05",
    "label": "Change presentation without resending",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c06",
    "label": "Keep draft when presentation changes",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c07",
    "label": "Open full conversation from a card",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c08",
    "label": "Fork from an earlier point",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c09",
    "label": "Fresh eyes / disconnect prior memory",
    "preferred": [
      "builder"
    ],
    "comment": "this should be a wire event. agent continuity is explisetly caried via a dedicated 4th wire for builder and immage generator tools sniping the wire wipes its memory for down stream",
    "undecided": false
  },
  {
    "id": "c10",
    "label": "Rename conversation / agent",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c11",
    "label": "Fold messages / transcript detail",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "c12",
    "label": "Interactive image mode toggle",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "i01",
    "label": "Editable prompt / composer",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "i02",
    "label": "Explicit source / context connection",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "i03",
    "label": "Named input roles / context chips",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "i04",
    "label": "Attach / remove a local image",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "i05",
    "label": "Send follow-up while work continues",
    "preferred": [
      "free"
    ],
    "comment": "shopuld be posible on builder",
    "undecided": false
  },
  {
    "id": "i06",
    "label": "Truthful steering / queued delivery",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "i07",
    "label": "Keep conversation draft on dismissal",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "i08",
    "label": "Create chat from source artifact",
    "preferred": [
      "artifact"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "a01",
    "label": "Attached generator editor / apron",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "a02",
    "label": "Browsable output / revision strip",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "a03",
    "label": "Select an earlier result",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "a04",
    "label": "Copy / Save as / place result",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "a05",
    "label": "Preview then Apply / Discard",
    "preferred": [],
    "comment": "none do thiswell",
    "undecided": true
  },
  {
    "id": "a06",
    "label": "Fork a revision / clone identity",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "a07",
    "label": "Inspect memory / open source",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "a08",
    "label": "Image count / aspect / seed",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "a09",
    "label": "Single / Live generation",
    "preferred": [
      "image"
    ],
    "comment": "a repeted issue with lableing throughout slate . the names single and live in isolation are meningles and confusingly misdirecting. ",
    "undecided": false
  },
  {
    "id": "a10",
    "label": "Generated text in document / note",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "r01",
    "label": "Explicit Run / Send action",
    "preferred": [
      "image",
      "chat",
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r02",
    "label": "Stop the active run",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r03",
    "label": "One Play control becomes Stop",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r04",
    "label": "Separate Pause / Resume and Stop",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r05",
    "label": "Preparing / running / error status",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r06",
    "label": "Streaming public commentary / tools",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r07",
    "label": "Expand activity: compact / partial / full",
    "preferred": [
      "image"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r08",
    "label": "Review earlier activity",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r09",
    "label": "Visible tool start / finish events",
    "preferred": [
      "free"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "r10",
    "label": "Elapsed time + time since last event",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "r11",
    "label": "Explicit quiet-provider waiting state",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "s01",
    "label": "Visible access / trust control",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "s02",
    "label": "Remember an access choice",
    "preferred": [],
    "comment": "i dont think any are globaly persistant",
    "undecided": true
  },
  {
    "id": "s03",
    "label": "One global access setting everywhere",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "s04",
    "label": "Current-run vs next-run permissions",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "s05",
    "label": "Allow / deny a provider request",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "s06",
    "label": "Provider access vs board Apply authority",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "s07",
    "label": "Missing-key entry flow",
    "preferred": [],
    "comment": "none do this well",
    "undecided": true
  },
  {
    "id": "s08",
    "label": "API chevron: Add / Change key",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "s09",
    "label": "Website / source access settings",
    "preferred": [
      "chat"
    ],
    "comment": "",
    "undecided": false
  },
  {
    "id": "f01",
    "label": "Roster / identity capsule",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "f02",
    "label": "Profile / avatar / hat picker",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "f03",
    "label": "Find agent on the board",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "f04",
    "label": "Clone agent with clear identity",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "f05",
    "label": "Schedule future work",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "f06",
    "label": "Retire / restore identity",
    "preferred": [],
    "comment": "",
    "undecided": true
  },
  {
    "id": "f07",
    "label": "Keep dialogue when harness changes",
    "preferred": [],
    "comment": "",
    "undecided": true
  }
]
  };
})();
