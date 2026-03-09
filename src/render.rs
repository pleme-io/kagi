//! GPU rendering module for vault browser UI.
//!
//! Uses madori (app framework) + garasu (GPU primitives) + egaku (widgets).
//!
//! ## Layout
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │  Search bar (TextInput)                  │
//! ├─────────────┬────────────────────────────┤
//! │ Vault list  │  Item list / Item detail   │
//! │ (ListView)  │  (ListView → detail view)  │
//! │             │                            │
//! │             │  Fields:                   │
//! │             │    username: user@...       │
//! │             │    password: ●●●●●●● [copy]│
//! │             │    url: https://...         │
//! └─────────────┴────────────────────────────┘
//! ```
//!
//! ## Rendering flow
//!
//! 1. madori handles window + event loop + frame timing
//! 2. Our RenderCallback implementation renders:
//!    - Background (Nord polar night)
//!    - Vault sidebar via egaku ListView
//!    - Item list or detail view
//!    - Search overlay when active
//!    - Text via garasu TextRenderer
//! 3. Input events dispatched to focused widget via egaku FocusManager
