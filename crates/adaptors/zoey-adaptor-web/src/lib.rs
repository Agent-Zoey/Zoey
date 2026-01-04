use axum::body;
use axum::body::Body;
use axum::extract::{Path, Request, State as AxumState};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::Html;
use axum::routing::any;
use axum::{routing::get, Router};
use zoey_core::utils::logger::{subscribe_logs, LogEvent};
use zoey_core::{AgentRuntime, Result};
use futures_util::stream::{BoxStream, StreamExt};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::{Arc, RwLock};
use tokio_stream::wrappers::BroadcastStream;
// no external time/uuid imports needed

/// Get the current character name from runtime state
fn get_current_character(state: &SimpleUiServer) -> String {
    let rt = state.runtime.read().unwrap();
    rt.character.name.clone()
}

/// Check if the current character is Zoey Lawyer
fn is_zoey_lawyer(character_name: &str) -> bool {
    let lower = character_name.to_lowercase();
    lower.contains("legal") && lower.contains("zoey")
}

/// Generate the Zoey Lawyer Case Management UI template
fn zoey_lawyer_template(_api_url: &str, token_js: &str, logs_js: &str) -> String {
    let template = r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Zoey Legal Assistant - Case Management</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
  <style>
    :root {
      --bg: #f8fafc;
      --card: #ffffff;
      --primary: #22d3ee;
      --secondary: #10b981;
      --text: #1e293b;
      --muted: #64748b;
      --border: rgba(0,0,0,0.08);
      --sidebar-bg: #f1f5f9;
      --hover: #e2e8f0;
      --danger: #ef4444;
      --warning: #f59e0b;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: 'Inter', system-ui, -apple-system, sans-serif;
      background: var(--bg);
      color: var(--text);
      min-height: 100vh;
    }
    
    /* Main Layout */
    .app-container {
      display: grid;
      grid-template-columns: 280px 1fr 300px;
      min-height: 100vh;
    }
    
    /* Left Sidebar - Cases */
    .case-sidebar {
      background: var(--sidebar-bg);
      border-right: 1px solid var(--border);
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
    .sidebar-header {
      padding: 20px;
      border-bottom: 1px solid var(--border);
      background: var(--card);
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 4px;
    }
    .brand-icon {
      width: 40px;
      height: 40px;
      border-radius: 10px;
      background: linear-gradient(135deg, var(--primary), var(--secondary));
      display: flex;
      align-items: center;
      justify-content: center;
      color: white;
      font-weight: 700;
      font-size: 18px;
    }
    .brand-text {
      font-weight: 700;
      font-size: 18px;
      color: var(--text);
    }
    .brand-subtitle {
      font-size: 12px;
      color: var(--muted);
      margin-left: 52px;
    }
    
    .new-case-btn {
      width: 100%;
      padding: 12px 16px;
      margin-top: 16px;
      border: 2px dashed var(--border);
      border-radius: 10px;
      background: transparent;
      color: var(--muted);
      font-weight: 600;
      font-size: 14px;
      cursor: pointer;
      transition: all 0.2s;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
    }
    .new-case-btn:hover {
      border-color: var(--primary);
      color: var(--primary);
      background: rgba(34, 211, 238, 0.05);
    }
    
    .case-list {
      flex: 1;
      overflow-y: auto;
      padding: 12px;
    }
    .case-section-title {
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--muted);
      padding: 8px 12px;
      margin-top: 8px;
    }
    .case-item {
      padding: 12px 14px;
      border-radius: 10px;
      cursor: pointer;
      transition: all 0.15s;
      margin-bottom: 4px;
      border: 1px solid transparent;
    }
    .case-item:hover {
      background: var(--hover);
    }
    .case-item.active {
      background: var(--card);
      border-color: var(--primary);
      box-shadow: 0 2px 8px rgba(34, 211, 238, 0.15);
    }
    .case-item-name {
      font-weight: 600;
      font-size: 14px;
      margin-bottom: 4px;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .case-item-meta {
      font-size: 12px;
      color: var(--muted);
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .case-status {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      font-size: 11px;
      font-weight: 500;
      padding: 2px 8px;
      border-radius: 12px;
    }
    .case-status.active {
      background: rgba(16, 185, 129, 0.1);
      color: #059669;
    }
    .case-status.closed {
      background: rgba(100, 116, 139, 0.1);
      color: var(--muted);
    }
    .closed-section {
      margin-top: 8px;
    }
    .closed-toggle {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      font-size: 12px;
      color: var(--muted);
      cursor: pointer;
      border-radius: 8px;
    }
    .closed-toggle:hover {
      background: var(--hover);
    }
    .closed-cases {
      display: none;
    }
    .closed-cases.expanded {
      display: block;
    }
    
    /* Main Chat Area */
    .main-content {
      display: flex;
      flex-direction: column;
      background: var(--bg);
    }
    .case-header {
      padding: 16px 24px;
      background: var(--card);
      border-bottom: 1px solid var(--border);
      display: flex;
      align-items: center;
      justify-content: space-between;
    }
    .case-title {
      font-size: 18px;
      font-weight: 700;
    }
    .case-header-actions {
      display: flex;
      gap: 10px;
    }
    .share-btn {
      padding: 8px 16px;
      border-radius: 8px;
      border: 1px solid var(--border);
      background: var(--card);
      color: var(--text);
      font-weight: 600;
      font-size: 13px;
      cursor: pointer;
      display: flex;
      align-items: center;
      gap: 6px;
      transition: all 0.15s;
    }
    .share-btn:hover {
      border-color: var(--primary);
      color: var(--primary);
    }
    
    .chat-container {
      flex: 1;
      overflow-y: auto;
      padding: 24px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }
    .welcome-message {
      text-align: center;
      padding: 60px 40px;
      color: var(--muted);
    }
    .welcome-icon {
      width: 80px;
      height: 80px;
      border-radius: 20px;
      background: linear-gradient(135deg, var(--primary), var(--secondary));
      display: flex;
      align-items: center;
      justify-content: center;
      margin: 0 auto 20px;
      font-size: 36px;
      color: white;
    }
    .welcome-title {
      font-size: 20px;
      font-weight: 700;
      color: var(--text);
      margin-bottom: 8px;
    }
    .welcome-subtitle {
      font-size: 14px;
      max-width: 400px;
      margin: 0 auto;
    }
    
    .msg {
      display: flex;
      gap: 12px;
      max-width: 85%;
    }
    .msg.user {
      margin-left: auto;
      flex-direction: row-reverse;
    }
    .msg-avatar {
      width: 36px;
      height: 36px;
      border-radius: 10px;
      display: flex;
      align-items: center;
      justify-content: center;
      font-weight: 600;
      font-size: 14px;
      flex-shrink: 0;
    }
    .msg.agent .msg-avatar {
      background: linear-gradient(135deg, var(--primary), var(--secondary));
      color: white;
    }
    .msg.user .msg-avatar {
      background: var(--hover);
      color: var(--muted);
    }
    .bubble {
      padding: 12px 16px;
      border-radius: 16px;
      font-size: 14px;
      line-height: 1.5;
      white-space: pre-wrap;
      word-break: break-word;
    }
    .msg.agent .bubble {
      background: var(--card);
      border: 1px solid var(--border);
      border-bottom-left-radius: 4px;
    }
    .msg.user .bubble {
      background: linear-gradient(135deg, var(--primary), var(--secondary));
      color: white;
      border-bottom-right-radius: 4px;
    }
    
    .input-container {
      padding: 16px 24px 24px;
      background: var(--card);
      border-top: 1px solid var(--border);
    }
    .input-wrap {
      display: flex;
      gap: 12px;
      align-items: flex-end;
    }
    .input-wrap input {
      flex: 1;
      padding: 14px 18px;
      border-radius: 12px;
      border: 1px solid var(--border);
      background: var(--bg);
      font-size: 14px;
      color: var(--text);
      outline: none;
      transition: all 0.15s;
    }
    .input-wrap input:focus {
      border-color: var(--primary);
      box-shadow: 0 0 0 3px rgba(34, 211, 238, 0.1);
    }
    .input-wrap input::placeholder {
      color: var(--muted);
    }
    .send-btn {
      padding: 14px 24px;
      border-radius: 12px;
      border: none;
      background: linear-gradient(135deg, var(--primary), var(--secondary));
      color: white;
      font-weight: 600;
      font-size: 14px;
      cursor: pointer;
      transition: all 0.15s;
    }
    .send-btn:hover {
      transform: translateY(-1px);
      box-shadow: 0 4px 12px rgba(34, 211, 238, 0.3);
    }
    .send-btn:disabled {
      opacity: 0.5;
      cursor: not-allowed;
      transform: none;
      box-shadow: none;
    }
    
    /* Right Sidebar - Case Details */
    .detail-sidebar {
      background: var(--card);
      border-left: 1px solid var(--border);
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
    .detail-header {
      padding: 20px;
      border-bottom: 1px solid var(--border);
      font-weight: 700;
      font-size: 14px;
    }
    .detail-content {
      flex: 1;
      overflow-y: auto;
      padding: 16px 20px;
    }
    .detail-section {
      margin-bottom: 24px;
    }
    .detail-section-title {
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--muted);
      margin-bottom: 12px;
    }
    .participant-list {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .participant {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 8px 10px;
      border-radius: 8px;
      background: var(--bg);
    }
    .participant-avatar {
      width: 32px;
      height: 32px;
      border-radius: 8px;
      background: var(--hover);
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 12px;
      font-weight: 600;
      color: var(--muted);
    }
    .participant-info {
      flex: 1;
    }
    .participant-name {
      font-size: 13px;
      font-weight: 600;
    }
    .participant-role {
      font-size: 11px;
      color: var(--muted);
    }
    
    .case-info-item {
      display: flex;
      justify-content: space-between;
      padding: 8px 0;
      border-bottom: 1px solid var(--border);
      font-size: 13px;
    }
    .case-info-label {
      color: var(--muted);
    }
    .case-info-value {
      font-weight: 500;
    }
    
    .action-buttons {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .action-btn {
      width: 100%;
      padding: 10px 14px;
      border-radius: 8px;
      border: 1px solid var(--border);
      background: var(--card);
      color: var(--text);
      font-weight: 600;
      font-size: 13px;
      cursor: pointer;
      transition: all 0.15s;
    }
    .action-btn:hover {
      background: var(--bg);
    }
    .action-btn.danger {
      border-color: rgba(239, 68, 68, 0.3);
      color: var(--danger);
    }
    .action-btn.danger:hover {
      background: rgba(239, 68, 68, 0.05);
    }
    
    /* Modal */
    .modal-overlay {
      position: fixed;
      inset: 0;
      background: rgba(0,0,0,0.5);
      display: none;
      align-items: center;
      justify-content: center;
      z-index: 1000;
    }
    .modal-overlay.active {
      display: flex;
    }
    .modal {
      background: var(--card);
      border-radius: 16px;
      padding: 24px;
      width: 100%;
      max-width: 420px;
      box-shadow: 0 20px 60px rgba(0,0,0,0.2);
    }
    .modal-title {
      font-size: 18px;
      font-weight: 700;
      margin-bottom: 16px;
    }
    .modal-input {
      width: 100%;
      padding: 12px 14px;
      border-radius: 10px;
      border: 1px solid var(--border);
      font-size: 14px;
      margin-bottom: 12px;
      outline: none;
    }
    .modal-input:focus {
      border-color: var(--primary);
    }
    .modal-actions {
      display: flex;
      gap: 10px;
      justify-content: flex-end;
      margin-top: 16px;
    }
    .modal-btn {
      padding: 10px 20px;
      border-radius: 8px;
      font-weight: 600;
      font-size: 14px;
      cursor: pointer;
      border: 1px solid var(--border);
      background: var(--card);
      color: var(--text);
    }
    .modal-btn.primary {
      background: linear-gradient(135deg, var(--primary), var(--secondary));
      color: white;
      border: none;
    }
    
    /* Share Modal */
    .share-link-container {
      display: flex;
      gap: 8px;
      margin-top: 8px;
    }
    .share-link-input {
      flex: 1;
      padding: 10px 14px;
      border-radius: 8px;
      border: 1px solid var(--border);
      background: var(--bg);
      font-size: 13px;
      color: var(--muted);
    }
    .copy-btn {
      padding: 10px 16px;
      border-radius: 8px;
      border: none;
      background: var(--primary);
      color: white;
      font-weight: 600;
      font-size: 13px;
      cursor: pointer;
    }
    
    /* Typing indicator */
    .typing-indicator {
      display: flex;
      gap: 4px;
      padding: 8px 12px;
    }
    .typing-indicator span {
      width: 8px;
      height: 8px;
      background: var(--muted);
      border-radius: 50%;
      animation: typing 1.2s infinite;
    }
    .typing-indicator span:nth-child(2) { animation-delay: 0.2s; }
    .typing-indicator span:nth-child(3) { animation-delay: 0.4s; }
    @keyframes typing {
      0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
      40% { opacity: 1; transform: scale(1); }
    }
    
    /* Empty state */
    .empty-state {
      text-align: center;
      padding: 40px 20px;
      color: var(--muted);
    }
    .empty-state-icon {
      font-size: 48px;
      margin-bottom: 16px;
      opacity: 0.5;
    }
    
    /* Toast notification */
    .toast {
      position: fixed;
      bottom: 24px;
      right: 24px;
      padding: 14px 20px;
      background: var(--text);
      color: white;
      border-radius: 10px;
      font-size: 14px;
      font-weight: 500;
      box-shadow: 0 4px 20px rgba(0,0,0,0.2);
      transform: translateY(100px);
      opacity: 0;
      transition: all 0.3s;
      z-index: 1001;
    }
    .toast.show {
      transform: translateY(0);
      opacity: 1;
    }
    
    /* File Drop Zone */
    .file-drop-section {
      padding: 12px;
      border-top: 1px solid var(--border);
      background: var(--card);
    }
    .file-drop-title {
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--muted);
      margin-bottom: 10px;
    }
    .file-drop-zone {
      border: 2px dashed var(--border);
      border-radius: 10px;
      padding: 20px 16px;
      text-align: center;
      cursor: pointer;
      transition: all 0.2s;
      background: var(--bg);
    }
    .file-drop-zone:hover, .file-drop-zone.dragover {
      border-color: var(--primary);
      background: rgba(34, 211, 238, 0.05);
    }
    .file-drop-zone.dragover {
      transform: scale(1.02);
    }
    .file-drop-icon {
      font-size: 28px;
      margin-bottom: 8px;
      opacity: 0.6;
    }
    .file-drop-text {
      font-size: 13px;
      color: var(--muted);
      margin-bottom: 4px;
    }
    .file-drop-hint {
      font-size: 11px;
      color: var(--muted);
      opacity: 0.7;
    }
    .file-list {
      margin-top: 12px;
      display: flex;
      flex-direction: column;
      gap: 6px;
      max-height: 150px;
      overflow-y: auto;
    }
    .file-item {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 10px;
      background: var(--bg);
      border-radius: 8px;
      font-size: 12px;
    }
    .file-item-icon {
      font-size: 16px;
    }
    .file-item-name {
      flex: 1;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .file-item-status {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: 4px;
      background: rgba(16, 185, 129, 0.1);
      color: #059669;
    }
    .file-item-status.uploading {
      background: rgba(245, 158, 11, 0.1);
      color: #d97706;
    }
    .file-item-remove {
      background: none;
      border: none;
      color: var(--muted);
      cursor: pointer;
      font-size: 14px;
      padding: 2px;
    }
    .file-item-remove:hover {
      color: var(--danger);
    }
    
    @media (max-width: 1100px) {
      .app-container {
        grid-template-columns: 1fr;
      }
      .case-sidebar, .detail-sidebar {
        display: none;
      }
    }
  </style>
</head>
<body>
  <div class="app-container">
    <!-- Left Sidebar - Cases -->
    <aside class="case-sidebar">
      <div class="sidebar-header">
        <div class="brand">
          <div class="brand-icon">Z</div>
          <div class="brand-text">Zoey</div>
        </div>
        <div class="brand-subtitle">Legal Case Assistant</div>
        <button class="new-case-btn" onclick="showNewCaseModal()">
          <span>+</span> New Case
        </button>
      </div>
      <div class="case-list" id="caseList">
        <div class="case-section-title">Active Cases</div>
        <div id="activeCases"></div>
        <div class="closed-section">
          <div class="closed-toggle" onclick="toggleClosedCases()">
            <span id="closedChevron">▸</span> Closed Cases
          </div>
          <div class="closed-cases" id="closedCases"></div>
        </div>
      </div>
      <!-- File Drop Zone -->
      <div class="file-drop-section" id="fileDropSection" style="display: none;">
        <div class="file-drop-title">Case Documents</div>
        <div class="file-drop-zone" id="fileDropZone">
          <div class="file-drop-icon">📄</div>
          <div class="file-drop-text">Drop files here</div>
          <div class="file-drop-hint">PDF, Excel, TXT, MD, CSV, JSON</div>
        </div>
        <input type="file" id="fileInput" multiple accept=".pdf,.xlsx,.xls,.txt,.md,.csv,.json" style="display: none;" />
        <div class="file-list" id="fileList"></div>
      </div>
    </aside>
    
    <!-- Main Chat Area -->
    <main class="main-content">
      <div class="case-header" id="caseHeader" style="display: none;">
        <div class="case-title" id="currentCaseTitle">No Case Selected</div>
        <div class="case-header-actions">
          <button class="share-btn" onclick="showShareModal()">
            <span>🔗</span> Share Case
          </button>
        </div>
      </div>
      <div class="chat-container" id="chat">
        <div class="welcome-message" id="welcomeMessage">
          <div class="welcome-icon">Z</div>
          <div class="welcome-title">Welcome to Zoey Legal Assistant</div>
          <div class="welcome-subtitle">Create a new case or select an existing one to start working with your AI legal assistant. All case data is isolated and secure.</div>
        </div>
      </div>
      <div class="input-container" id="inputContainer" style="display: none;">
        <div class="input-wrap">
          <input type="text" id="messageInput" placeholder="Ask Zoey about this case..." />
          <button class="send-btn" id="sendBtn" onclick="sendMessage()">Send</button>
        </div>
      </div>
    </main>
    
    <!-- Right Sidebar - Case Details -->
    <aside class="detail-sidebar" id="detailSidebar" style="display: none;">
      <div class="detail-header">Case Details</div>
      <div class="detail-content">
        <div class="detail-section">
          <div class="detail-section-title">Participants</div>
          <div class="participant-list" id="participantList">
            <div class="participant">
              <div class="participant-avatar">Y</div>
              <div class="participant-info">
                <div class="participant-name">You</div>
                <div class="participant-role">Owner</div>
              </div>
            </div>
          </div>
        </div>
        <div class="detail-section">
          <div class="detail-section-title">Case Information</div>
          <div id="caseInfo">
            <div class="case-info-item">
              <span class="case-info-label">Created</span>
              <span class="case-info-value" id="caseCreated">-</span>
            </div>
            <div class="case-info-item">
              <span class="case-info-label">Messages</span>
              <span class="case-info-value" id="caseMessages">0</span>
            </div>
            <div class="case-info-item">
              <span class="case-info-label">Status</span>
              <span class="case-info-value" id="caseStatus">Active</span>
            </div>
          </div>
        </div>
        <div class="detail-section">
          <div class="detail-section-title">Actions</div>
          <div class="action-buttons">
            <button class="action-btn" onclick="closeCaseAction()">Close Case</button>
            <button class="action-btn danger" onclick="deleteCaseAction()">Delete Case Data</button>
          </div>
        </div>
      </div>
    </aside>
  </div>
  
  <!-- New Case Modal -->
  <div class="modal-overlay" id="newCaseModal">
    <div class="modal">
      <div class="modal-title">Create New Case</div>
      <input type="text" class="modal-input" id="newCaseName" placeholder="Case name (e.g., Smith v. ACME Corp)" />
      <input type="text" class="modal-input" id="newCaseMatter" placeholder="Matter number (optional)" />
      <div class="modal-actions">
        <button class="modal-btn" onclick="hideNewCaseModal()">Cancel</button>
        <button class="modal-btn primary" onclick="createCase()">Create Case</button>
      </div>
    </div>
  </div>
  
  <!-- Share Modal -->
  <div class="modal-overlay" id="shareModal">
    <div class="modal">
      <div class="modal-title">Share Case</div>
      <p style="color: var(--muted); font-size: 14px; margin-bottom: 12px;">Share this link with others to invite them to this case:</p>
      <div class="share-link-container">
        <input type="text" class="share-link-input" id="shareLink" readonly />
        <button class="copy-btn" onclick="copyShareLink()">Copy</button>
      </div>
      <div class="modal-actions">
        <button class="modal-btn" onclick="hideShareModal()">Close</button>
      </div>
    </div>
  </div>
  
  <!-- Toast -->
  <div class="toast" id="toast"></div>
  
  <script>
    const API = '/agent';
    {TOKEN_JS}
    {LOGS_JS}
    
    // Entity ID (user identifier)
    const entityId = localStorage.getItem('zoey_entity') || uuid();
    localStorage.setItem('zoey_entity', entityId);
    
    // Case state
    let cases = JSON.parse(localStorage.getItem('zoey_cases') || '[]');
    let activeCase = null;
    let messageCount = 0;
    
    function uuid() {
      try { if (crypto?.randomUUID) return crypto.randomUUID(); } catch {}
      const rv = () => crypto?.getRandomValues ? (crypto.getRandomValues(new Uint8Array(1))[0] & 15) : (Math.random() * 16 | 0);
      return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, c => {
        const v = rv();
        return (c === 'x' ? v : (v & 0x3 | 0x8)).toString(16);
      });
    }
    
    function showToast(message) {
      const toast = document.getElementById('toast');
      toast.textContent = message;
      toast.classList.add('show');
      setTimeout(() => toast.classList.remove('show'), 3000);
    }
    
    function saveCases() {
      localStorage.setItem('zoey_cases', JSON.stringify(cases));
    }
    
    function formatDate(timestamp) {
      const d = new Date(timestamp);
      const now = new Date();
      const diffMs = now - d;
      const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
      if (diffDays === 0) return 'Today';
      if (diffDays === 1) return 'Yesterday';
      if (diffDays < 7) return `${diffDays} days ago`;
      return d.toLocaleDateString();
    }
    
    function renderCaseList() {
      const activeCasesEl = document.getElementById('activeCases');
      const closedCasesEl = document.getElementById('closedCases');
      
      const activeCases = cases.filter(c => c.status === 'active');
      const closedCases = cases.filter(c => c.status === 'closed');
      
      activeCasesEl.innerHTML = activeCases.length === 0 
        ? '<div class="empty-state"><div style="font-size: 14px;">No active cases</div></div>'
        : activeCases.map(c => `
          <div class="case-item ${activeCase?.id === c.id ? 'active' : ''}" onclick="selectCase('${c.id}')">
            <div class="case-item-name">${escapeHtml(c.name)}</div>
            <div class="case-item-meta">
              <span class="case-status active">Active</span>
              <span>${formatDate(c.lastActivity || c.createdAt)}</span>
            </div>
          </div>
        `).join('');
      
      closedCasesEl.innerHTML = closedCases.length === 0
        ? '<div class="empty-state" style="padding: 16px;"><div style="font-size: 13px;">No closed cases</div></div>'
        : closedCases.map(c => `
          <div class="case-item ${activeCase?.id === c.id ? 'active' : ''}" onclick="selectCase('${c.id}')">
            <div class="case-item-name">${escapeHtml(c.name)}</div>
            <div class="case-item-meta">
              <span class="case-status closed">Closed</span>
            </div>
          </div>
        `).join('');
    }
    
    function escapeHtml(text) {
      const div = document.createElement('div');
      div.textContent = text;
      return div.innerHTML;
    }
    
    function selectCase(caseId) {
      const c = cases.find(x => x.id === caseId);
      if (!c) return;
      
      activeCase = c;
      document.getElementById('caseHeader').style.display = 'flex';
      document.getElementById('inputContainer').style.display = 'block';
      document.getElementById('detailSidebar').style.display = 'flex';
      document.getElementById('welcomeMessage').style.display = 'none';
      document.getElementById('currentCaseTitle').textContent = c.name;
      document.getElementById('caseCreated').textContent = formatDate(c.createdAt);
      document.getElementById('caseMessages').textContent = c.messageCount || 0;
      document.getElementById('caseStatus').textContent = c.status === 'active' ? 'Active' : 'Closed';
      
      // Load case messages from localStorage
      const messagesKey = `zoey_case_messages_${caseId}`;
      const messages = JSON.parse(localStorage.getItem(messagesKey) || '[]');
      renderMessages(messages);
      messageCount = messages.length;
      
      renderCaseList();
    }
    
    function renderMessages(messages) {
      const chat = document.getElementById('chat');
      if (messages.length === 0) {
        chat.innerHTML = `
          <div class="welcome-message">
            <div class="welcome-icon">Z</div>
            <div class="welcome-title">Case: ${escapeHtml(activeCase.name)}</div>
            <div class="welcome-subtitle">Start chatting with Zoey about this case. All conversations are private to this case.</div>
          </div>
        `;
        return;
      }
      chat.innerHTML = messages.map(m => `
        <div class="msg ${m.role}">
          <div class="msg-avatar">${m.role === 'agent' ? 'Z' : 'Y'}</div>
          <div class="bubble">${escapeHtml(m.text)}</div>
        </div>
      `).join('');
      chat.scrollTop = chat.scrollHeight;
    }
    
    function addMessage(role, text) {
      if (!activeCase) return;
      const messagesKey = `zoey_case_messages_${activeCase.id}`;
      const messages = JSON.parse(localStorage.getItem(messagesKey) || '[]');
      messages.push({ role, text, timestamp: Date.now() });
      localStorage.setItem(messagesKey, JSON.stringify(messages));
      
      activeCase.messageCount = messages.length;
      activeCase.lastActivity = Date.now();
      saveCases();
      document.getElementById('caseMessages').textContent = messages.length;
      
      const chat = document.getElementById('chat');
      const welcomeMsg = chat.querySelector('.welcome-message');
      if (welcomeMsg) welcomeMsg.remove();
      
      const msgEl = document.createElement('div');
      msgEl.className = `msg ${role}`;
      msgEl.innerHTML = `
        <div class="msg-avatar">${role === 'agent' ? 'Z' : 'Y'}</div>
        <div class="bubble">${escapeHtml(text)}</div>
      `;
      chat.appendChild(msgEl);
      chat.scrollTop = chat.scrollHeight;
    }
    
    function showTyping() {
      const chat = document.getElementById('chat');
      let typing = document.getElementById('typingIndicator');
      if (!typing) {
        typing = document.createElement('div');
        typing.id = 'typingIndicator';
        typing.className = 'msg agent';
        typing.innerHTML = `
          <div class="msg-avatar">Z</div>
          <div class="bubble"><div class="typing-indicator"><span></span><span></span><span></span></div></div>
        `;
        chat.appendChild(typing);
      }
      chat.scrollTop = chat.scrollHeight;
    }
    
    function hideTyping() {
      const typing = document.getElementById('typingIndicator');
      if (typing) typing.remove();
    }
    
    async function sendMessage() {
      if (!activeCase) {
        showToast('Please select or create a case first');
        return;
      }
      
      const input = document.getElementById('messageInput');
      const text = input.value.trim();
      if (!text) return;
      
      input.value = '';
      addMessage('user', text);
      showTyping();
      
      try {
        const headers = { 'Content-Type': 'application/json' };
        if (TOKEN) headers['Authorization'] = 'Bearer ' + TOKEN;
        
        const res = await fetch(API + '/chat/stream', {
          method: 'POST',
          headers,
          body: JSON.stringify({
            text,
            roomId: activeCase.id,
            entityId,
            stream: true
          })
        });
        
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let assembled = '';
        
        while (true) {
          const { value, done } = await reader.read();
          if (done) break;
          
          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split('\n');
          buffer = lines.pop();
          
          for (const line of lines) {
            if (line.startsWith('data:')) {
              try {
                const payload = JSON.parse(line.slice(5));
                if (payload.text) {
                  assembled += payload.text;
                }
              } catch {}
            }
          }
        }
        
        hideTyping();
        if (assembled.trim()) {
          // Parse reply from response
          const reply = parseReply(assembled);
          addMessage('agent', reply);
        } else {
          addMessage('agent', 'I apologize, but I couldn\'t process your request. Please try again.');
        }
      } catch (e) {
        hideTyping();
        addMessage('agent', 'Connection error. Please try again.');
      }
    }
    
    function parseReply(text) {
      // Try to extract reply from various formats
      const replyMatch = text.match(/<reply>([\s\S]*?)<\/reply>/i);
      if (replyMatch) return replyMatch[1].trim();
      
      const textMatch = text.match(/<text>([\s\S]*?)<\/text>/i);
      if (textMatch) return textMatch[1].trim();
      
      return text.trim();
    }
    
    // New Case Modal
    function showNewCaseModal() {
      document.getElementById('newCaseModal').classList.add('active');
      document.getElementById('newCaseName').focus();
    }
    
    function hideNewCaseModal() {
      document.getElementById('newCaseModal').classList.remove('active');
      document.getElementById('newCaseName').value = '';
      document.getElementById('newCaseMatter').value = '';
    }
    
    function createCase() {
      const name = document.getElementById('newCaseName').value.trim();
      const matter = document.getElementById('newCaseMatter').value.trim();
      
      if (!name) {
        showToast('Please enter a case name');
        return;
      }
      
      const newCase = {
        id: uuid(),
        name,
        matterNumber: matter || null,
        status: 'active',
        isOwner: true,
        inviteToken: uuid().replace(/-/g, '').slice(0, 16),
        createdAt: Date.now(),
        lastActivity: Date.now(),
        messageCount: 0
      };
      
      cases.unshift(newCase);
      saveCases();
      hideNewCaseModal();
      selectCase(newCase.id);
      showToast('Case created successfully');
    }
    
    // Share Modal
    function showShareModal() {
      if (!activeCase) return;
      const url = `${window.location.origin}${window.location.pathname}?case=${activeCase.id}&invite=${activeCase.inviteToken}`;
      document.getElementById('shareLink').value = url;
      document.getElementById('shareModal').classList.add('active');
    }
    
    function hideShareModal() {
      document.getElementById('shareModal').classList.remove('active');
    }
    
    function copyShareLink() {
      const input = document.getElementById('shareLink');
      input.select();
      navigator.clipboard.writeText(input.value);
      showToast('Link copied to clipboard');
    }
    
    // Closed cases toggle
    function toggleClosedCases() {
      const closedCases = document.getElementById('closedCases');
      const chevron = document.getElementById('closedChevron');
      closedCases.classList.toggle('expanded');
      chevron.textContent = closedCases.classList.contains('expanded') ? '▾' : '▸';
    }
    
    // Case actions
    function closeCaseAction() {
      if (!activeCase || !activeCase.isOwner) {
        showToast('Only the case owner can close this case');
        return;
      }
      if (confirm('Are you sure you want to close this case? It can be reopened later.')) {
        activeCase.status = 'closed';
        saveCases();
        renderCaseList();
        document.getElementById('caseStatus').textContent = 'Closed';
        showToast('Case closed');
      }
    }
    
    async function deleteCaseAction() {
      if (!activeCase || !activeCase.isOwner) {
        showToast('Only the case owner can delete case data');
        return;
      }
      if (confirm('Are you sure you want to DELETE all data for this case? This action cannot be undone.')) {
        try {
          const headers = { 'Content-Type': 'application/json' };
          if (TOKEN) headers['Authorization'] = 'Bearer ' + TOKEN;
          
          await fetch(API + '/room/delete', {
            method: 'POST',
            headers,
            body: JSON.stringify({
              room_id: activeCase.id,
              entity_id: entityId,
              purge_memories: true
            })
          });
          
          // Remove local data
          localStorage.removeItem(`zoey_case_messages_${activeCase.id}`);
          cases = cases.filter(c => c.id !== activeCase.id);
          saveCases();
          
          activeCase = null;
          document.getElementById('caseHeader').style.display = 'none';
          document.getElementById('inputContainer').style.display = 'none';
          document.getElementById('detailSidebar').style.display = 'none';
          document.getElementById('welcomeMessage').style.display = 'block';
          document.getElementById('chat').innerHTML = document.getElementById('welcomeMessage').outerHTML;
          
          renderCaseList();
          showToast('Case data deleted');
        } catch (e) {
          showToast('Failed to delete case data');
        }
      }
    }
    
    // Handle invite links
    function handleInviteLink() {
      const params = new URLSearchParams(window.location.search);
      const caseId = params.get('case');
      const inviteToken = params.get('invite');
      
      if (caseId && inviteToken) {
        // Check if we already have this case
        let existingCase = cases.find(c => c.id === caseId);
        
        if (!existingCase) {
          // Add as invited case
          existingCase = {
            id: caseId,
            name: 'Shared Case',
            status: 'active',
            isOwner: false,
            inviteToken,
            createdAt: Date.now(),
            lastActivity: Date.now(),
            messageCount: 0
          };
          cases.push(existingCase);
          saveCases();
          showToast('Joined shared case');
        }
        
        selectCase(caseId);
        // Clean URL
        window.history.replaceState({}, document.title, window.location.pathname);
      }
    }
    
    // Enter key handler
    document.getElementById('messageInput').addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        sendMessage();
      }
    });
    
    // ========== FILE UPLOAD FUNCTIONALITY ==========
    
    // Case files storage
    function getCaseFiles(caseId) {
      return JSON.parse(localStorage.getItem(`zoey_case_files_${caseId}`) || '[]');
    }
    
    function saveCaseFiles(caseId, files) {
      localStorage.setItem(`zoey_case_files_${caseId}`, JSON.stringify(files));
    }
    
    // File type detection
    function getFileType(filename) {
      const ext = filename.split('.').pop().toLowerCase();
      const typeMap = {
        'pdf': 'PDF', 'txt': 'Text', 'md': 'Markdown',
        'csv': 'CSV', 'json': 'JSON', 'doc': 'Document', 'docx': 'Document'
      };
      return typeMap[ext] || 'File';
    }
    
    function getFileIcon(filename) {
      const ext = filename.split('.').pop().toLowerCase();
      const iconMap = {
        'pdf': '📕', 'txt': '📄', 'md': '📝',
        'csv': '📊', 'json': '📋', 'xlsx': '📗', 'xls': '📗'
      };
      return iconMap[ext] || '📄';
    }
    
    // Render file list with ingestion status
    function renderFileList() {
      const fileList = document.getElementById('fileList');
      if (!activeCase || !fileList) return;
      
      const files = getCaseFiles(activeCase.id);
      
      if (files.length === 0) {
        fileList.innerHTML = '';
        return;
      }
      
      fileList.innerHTML = files.map((f, idx) => {
        // Determine status badge style and text
        let statusClass = '';
        let statusText = getFileType(f.name);
        if (f.status === 'uploading') {
          statusClass = ' uploading';
          statusText = 'Processing...';
        } else if (f.status === 'ingested') {
          statusClass = '';
          statusText = f.chunksCreated ? `${f.chunksCreated} chunks` : 'Ingested';
        } else if (f.status === 'error') {
          statusClass = ' error';
          statusText = 'Error';
        }
        
        return `
          <div class="file-item">
            <span class="file-item-icon">${getFileIcon(f.name)}</span>
            <span class="file-item-name" title="${escapeHtml(f.name)}${f.wordCount ? ' (' + f.wordCount + ' words)' : ''}">${escapeHtml(f.name)}</span>
            <span class="file-item-status${statusClass}">${statusText}</span>
            <button class="file-item-remove" onclick="removeFile(${idx})" title="Remove">×</button>
          </div>
        `;
      }).join('');
    }
    
    // Remove file
    function removeFile(idx) {
      if (!activeCase) return;
      const files = getCaseFiles(activeCase.id);
      files.splice(idx, 1);
      saveCaseFiles(activeCase.id, files);
      renderFileList();
      showToast('File removed');
    }
    
    // Upload file to Knowledge Ingestion API (secure document processing)
    async function uploadFile(file) {
      if (!activeCase) {
        showToast('Please select a case first');
        return;
      }
      
      // Validate file type - now includes PDF and Excel
      const textExtensions = ['txt', 'md', 'markdown', 'csv', 'json'];
      const binaryExtensions = ['pdf', 'xlsx', 'xls'];
      const allowedExtensions = [...textExtensions, ...binaryExtensions];
      const ext = file.name.split('.').pop()?.toLowerCase() || '';
      if (!allowedExtensions.includes(ext)) {
        showToast(`Unsupported file type: .${ext}. Allowed: .txt, .md, .csv, .json, .pdf, .xlsx, .xls`);
        return Promise.reject(new Error('Unsupported file type'));
      }
      
      // Validate file size (10MB max)
      const maxSize = 10 * 1024 * 1024;
      if (file.size > maxSize) {
        showToast('File too large (max 10MB)');
        return Promise.reject(new Error('File too large'));
      }
      
      // Determine if file needs base64 encoding (binary files)
      const isBinary = binaryExtensions.includes(ext);
      
      return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = async (e) => {
          let content;
          let base64Encoded = false;
          
          if (isBinary) {
            // For binary files, encode as base64
            const arrayBuffer = e.target.result;
            const bytes = new Uint8Array(arrayBuffer);
            let binary = '';
            for (let i = 0; i < bytes.byteLength; i++) {
              binary += String.fromCharCode(bytes[i]);
            }
            content = btoa(binary);
            base64Encoded = true;
          } else {
            // For text files, use as-is
            content = e.target.result;
          }
          
          // Store file metadata locally (will update with server response)
          const files = getCaseFiles(activeCase.id);
          const fileRecord = {
            id: uuid(),
            name: file.name,
            type: file.type,
            size: file.size,
            uploadedAt: Date.now(),
            status: 'uploading'
          };
          files.push(fileRecord);
          saveCaseFiles(activeCase.id, files);
          renderFileList();
          
          // Send file to Knowledge Ingestion endpoint
          try {
            const headers = { 'Content-Type': 'application/json' };
            if (TOKEN) headers['Authorization'] = 'Bearer ' + TOKEN;
            
            // Use the secure knowledge ingestion endpoint
            const response = await fetch(API + '/knowledge/ingest', {
              method: 'POST',
              headers,
              body: JSON.stringify({
                room_id: activeCase.id,
                entity_id: entityId,
                filename: file.name,
                content: content,
                base64_encoded: base64Encoded,
                mime_type: file.type || (isBinary ? 'application/octet-stream' : 'text/plain'),
                metadata: {
                  original_size: file.size,
                  upload_timestamp: Date.now(),
                  client_id: fileRecord.id
                }
              })
            });
            
            const result = await response.json();
            
            if (result.success) {
              // Update file record with server info
              const updatedFiles = getCaseFiles(activeCase.id);
              const idx = updatedFiles.findIndex(f => f.id === fileRecord.id);
              if (idx !== -1) {
                updatedFiles[idx].status = 'ingested';
                updatedFiles[idx].documentId = result.documentId;
                updatedFiles[idx].chunksCreated = result.chunksCreated;
                updatedFiles[idx].wordCount = result.wordCount;
                saveCaseFiles(activeCase.id, updatedFiles);
                renderFileList();
              }
              
              // Show success with details
              let msg = `Ingested: ${file.name}`;
              if (result.chunksCreated) msg += ` (${result.chunksCreated} chunks)`;
              if (result.warnings && result.warnings.length > 0) {
                msg += ` - Note: ${result.warnings[0]}`;
              }
              showToast(msg);
              resolve(fileRecord);
            } else {
              // Remove failed file from list
              const updatedFiles = getCaseFiles(activeCase.id).filter(f => f.id !== fileRecord.id);
              saveCaseFiles(activeCase.id, updatedFiles);
              renderFileList();
              
              showToast(`Failed: ${result.error || 'Unknown error'}`);
              reject(new Error(result.error || 'Upload failed'));
            }
          } catch (err) {
            // Remove failed file from list
            const updatedFiles = getCaseFiles(activeCase.id).filter(f => f.id !== fileRecord.id);
            saveCaseFiles(activeCase.id, updatedFiles);
            renderFileList();
            
            showToast('Upload error: ' + (err.message || 'Connection failed'));
            reject(err);
          }
        };
        reader.onerror = () => reject(reader.error);
        
        // Read as appropriate type
        if (isBinary) {
          reader.readAsArrayBuffer(file);
        } else {
          reader.readAsText(file);
        }
      });
    }
    
    // Handle file drop
    function setupFileDropZone() {
      const dropZone = document.getElementById('fileDropZone');
      const fileInput = document.getElementById('fileInput');
      
      if (!dropZone || !fileInput) return;
      
      // Click to select files
      dropZone.addEventListener('click', () => {
        if (activeCase) fileInput.click();
        else showToast('Please select a case first');
      });
      
      // File input change
      fileInput.addEventListener('change', async (e) => {
        const files = Array.from(e.target.files);
        for (const file of files) {
          await uploadFile(file);
        }
        fileInput.value = '';
      });
      
      // Drag and drop events
      dropZone.addEventListener('dragover', (e) => {
        e.preventDefault();
        dropZone.classList.add('dragover');
      });
      
      dropZone.addEventListener('dragleave', (e) => {
        e.preventDefault();
        dropZone.classList.remove('dragover');
      });
      
      dropZone.addEventListener('drop', async (e) => {
        e.preventDefault();
        dropZone.classList.remove('dragover');
        
        if (!activeCase) {
          showToast('Please select a case first');
          return;
        }
        
        const files = Array.from(e.dataTransfer.files);
        for (const file of files) {
          await uploadFile(file);
        }
      });
    }
    
    // Show/hide file section when case is selected
    function updateFileSectionVisibility() {
      const fileSection = document.getElementById('fileDropSection');
      if (fileSection) {
        fileSection.style.display = activeCase ? 'block' : 'none';
      }
    }
    
    // Patch selectCase to show files
    const originalSelectCase = selectCase;
    selectCase = function(caseId) {
      originalSelectCase(caseId);
      updateFileSectionVisibility();
      renderFileList();
    };
    
    // Initialize
    renderCaseList();
    handleInviteLink();
    setupFileDropZone();
  </script>
</body>
</html>"##;
    
    template
        .replace("{TOKEN_JS}", token_js)
        .replace("{LOGS_JS}", logs_js)
}

#[derive(Clone)]
pub struct SimpleUiConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub agent_api_url: String,
    pub use_streaming: bool,
    pub token: Option<String>,
    pub logs_enabled: bool,
}

impl Default for SimpleUiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".into(),
            port: 4000,
            agent_api_url: "http://127.0.0.1:9090/agent".into(),
            use_streaming: false,
            token: None,
            logs_enabled: false,
        }
    }
}

#[derive(Clone)]
pub struct SimpleUiServer {
    pub config: Arc<SimpleUiConfig>,
    pub runtime: Arc<RwLock<AgentRuntime>>,
}

#[derive(Deserialize)]
pub struct ChatInput {
    pub text: String,
}

#[derive(Serialize)]
pub struct ChatOutput {
    pub success: bool,
    pub messages: Vec<String>,
}

impl SimpleUiServer {
    pub fn new(config: SimpleUiConfig, runtime: Arc<RwLock<AgentRuntime>>) -> Self {
        Self {
            config: Arc::new(config),
            runtime,
        }
    }

    fn router(&self) -> Router {
        let mut r = Router::new()
            .route("/", get(index))
            // Proxy all /agent/... calls to configured Agent API backend
            .route("/agent/*rest", any(agent_proxy))
            .with_state(self.clone());
        if self.config.logs_enabled {
            r = r.route("/logs", get(ui_logs_sse));
        }
        r
    }

    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        let router = self.router();
        tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await
        });
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

async fn index(axum::extract::State(state): axum::extract::State<SimpleUiServer>) -> Html<String> {
    let api_url = &state.config.agent_api_url;
    let use_streaming = state.config.use_streaming;
    let token_js = match &state.config.token {
        Some(t) => format!("const TOKEN = '{}';", t),
        None => "const TOKEN = localStorage.getItem('zoey_token') || '';".to_string(),
    };
    let logs_js = if state.config.logs_enabled {
        "const LOGS_ENABLED = true;".to_string()
    } else {
        "const LOGS_ENABLED = false;".to_string()
    };
    
    // Check if current character is Zoey Lawyer - serve specialized case management UI
    let character_name = get_current_character(&state);
    if is_zoey_lawyer(&character_name) {
        return Html(zoey_lawyer_template(api_url, &token_js, &logs_js));
    }
    
    // Default generic chat UI for other characters
    let template = r#"<!doctype html><html><head><meta charset='utf-8'><title>ZoeyAI Tester</title>
    <style>
      :root { --bg:#0f172a; --panel:#111827; --accent:#22d3ee; --text:#e5e7eb; --muted:#94a3b8; --agent:#10b981; }
      body { margin:0; background: radial-gradient(1200px 600px at 10% 10%, #0b1220 0%, #0f172a 60%, #0b1020 100%); color:var(--text); font-family: Inter, system-ui, -apple-system, Segoe UI, Roboto, sans-serif; }
      .wrap { display:grid; grid-template-columns: 1fr 320px; gap:24px; padding:24px; }
      header { grid-column: 1 / -1; display:flex; align-items:center; justify-content:space-between; padding:16px 20px; background: rgba(255,255,255,0.03); border:1px solid rgba(255,255,255,0.08); border-radius:12px; backdrop-filter:saturate(120%) blur(6px); }
      .brand { display:flex; align-items:center; gap:12px; font-weight:600; letter-spacing:.3px; }
      .dot { width:10px; height:10px; border-radius:50%; background:var(--agent); box-shadow:0 0 12px var(--agent); }
      .chat { background: rgba(255,255,255,0.03); border:1px solid rgba(255,255,255,0.08); border-radius:12px; padding:16px; min-height:420px; display:flex; flex-direction:column; gap:8px; overflow-y: auto; }
      .msg { display:flex; gap:10px; align-items:flex-start; }
      .bubble { max-width: 68ch; padding:10px 12px; border-radius:14px; line-height:1.4; font-size:15px; white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word; }
      .user .bubble { background:#1f2937; border:1px solid #374151; }
      .agent .bubble { background:#0b2a22; border:1px solid #134e4a; }
      .thoughts { margin-top:6px; padding:8px 10px; border-left:3px solid var(--accent); background:rgba(34, 211, 238, .08); color:#a5f3fc; border-radius:8px; font-size:13px; }
      .input { display:flex; gap:10px; margin-top:10px; }
      .input input { flex:1; padding:12px 14px; border-radius:10px; border:1px solid rgba(255,255,255,0.1); background:#0b1220; color:var(--text); }
      .input button { padding:12px 18px; border-radius:10px; border:0; background:linear-gradient(90deg, #22d3ee, #10b981); color:#051018; font-weight:600; cursor:pointer; }
      .panel { background: rgba(255,255,255,0.03); border:1px solid rgba(255,255,255,0.08); border-radius:12px; padding:16px; }
      .muted { color:var(--muted); font-size:13px; }
      .typing { display:inline-block; }
      .typing span { display:inline-block; width:6px; height:6px; margin-right:4px; background:var(--muted); border-radius:50%; animation: blink 1.2s infinite; }
      .typing span:nth-child(2) { animation-delay: .2s }
      .typing span:nth-child(3) { animation-delay: .4s }
      @keyframes blink { 0%, 80%, 100% { opacity:.2 } 40% { opacity:1 } }
      @media (max-width: 980px) { .wrap { grid-template-columns: 1fr } }
    </style>
    </head>
    <body>
      <div class="wrap">
        <header>
          <div class="brand"><div class="dot"></div> Zoey Simple UI</div>
          <div style="display:flex; gap:10px; align-items:center;">
            <select id="character" style="background:#0b1220; color:var(--text); border:1px solid rgba(255,255,255,.1); border-radius:8px; padding:8px 10px;"></select>
            <button id="applyChar" style="padding:8px 12px; border-radius:8px; border:0; background:linear-gradient(90deg, #22d3ee, #10b981); color:#051018; font-weight:600; cursor:pointer;">Use Character</button>
            
          </div>
        </header>
        <div class="chat" id="chat"></div>
        <aside class="panel">
          <div style="font-weight:600; margin-bottom:8px;">Session</div>
          <div class="muted" id="session">Room: <span id="room"></span></div>
          <div style="font-weight:600; margin:12px 0 8px;">Agent State</div>
          <div class="muted" id="state">Compose after first reply…</div>
          
          <div style="font-weight:600; margin:12px 0 8px;">Thought Chain</div>
          <div id="chain" class="muted" style="display:flex; flex-direction:column; gap:8px;"></div>
          <div style="font-weight:600; margin:12px 0 8px;">Runtime Logs</div>
          <div id="logs" class="muted" style="display:flex; flex-direction:column; gap:6px; max-height:180px; overflow:auto; border:1px solid rgba(255,255,255,0.08); border-radius:8px; padding:8px;"></div>
          <div style="display:flex; gap:8px; margin-top:6px;">
            <button id="clearLogs" style="padding:6px 10px; border-radius:8px; border:0; background:#1f2937; color:#9ca3af; font-weight:600; cursor:pointer;">Clear</button>
            <button id="copyLogs" style="padding:6px 10px; border-radius:8px; border:0; background:linear-gradient(90deg, #22d3ee, #10b981); color:#051018; font-weight:600; cursor:pointer;">Copy</button>
          </div>
        </aside>
        <div class="input" style="grid-column: 1 / -1">
          <input id="t" placeholder="Ask Zoey anything…" />
          <button id="send">Send</button>
        </div>
      </div>
      <script>
        const API = '/agent';
        {TOKEN_JS}
        {LOGS_JS}
        const chat = document.getElementById('chat');
        const input = document.getElementById('t');
        const btn = document.getElementById('send');
        
        function uuid(){
          try{ if (typeof crypto!== 'undefined' && crypto && typeof crypto.randomUUID==='function') { return crypto.randomUUID(); } }catch(e){}
          function rv(){ if (typeof crypto!== 'undefined' && crypto && typeof crypto.getRandomValues==='function') { return (crypto.getRandomValues(new Uint8Array(1))[0] & 15); } return (Math.floor(Math.random()*16) & 15); }
          return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c){ const v = rv(); const n = c==='x' ? v : ((v & 0x3) | 0x8); return n.toString(16); });
        }
        const roomId = uuid();
        const entityId = (localStorage.getItem('zoey_entity') || uuid());
        localStorage.setItem('zoey_entity', entityId);
        document.getElementById('room').textContent = roomId;
        const charSelect = document.getElementById('character');
        const applyChar = document.getElementById('applyChar');
        const logsEl = document.getElementById('logs');
        const logsBuf = [];
        function ts(){ const d=new Date(); return d.toISOString().split('T')[1].replace('Z',''); }
        function addLog(level, msg){ const line = '['+ts()+'] '+String(level||'info').toUpperCase()+': '+String(msg); logsBuf.push(line); if (logsBuf.length>500) logsBuf.shift(); if (logsEl){ const item=document.createElement('div'); item.textContent=line; logsEl.appendChild(item); logsEl.scrollTop = logsEl.scrollHeight; } }
        async function fetchWithLog(url, opts, tag){
          addLog('info', 'Request ' + (tag||'') + ' ' + url);
          try {
            const res = await fetch(url, opts);
            addLog('info', 'Response ' + (tag||'') + ' ' + res.status);
            return res;
          } catch(e) {
            addLog('error', 'Fetch ' + (tag||'') + ' failed');
            return new Response(null, { status: 0, statusText: 'network_error' });
          }
        }
        document.getElementById('clearLogs').addEventListener('click', ()=>{ logsBuf.length=0; if (logsEl) logsEl.innerHTML=''; });
        document.getElementById('copyLogs').addEventListener('click', async ()=>{ try { await navigator.clipboard.writeText(logsBuf.join('\n')); addLog('info','Logs copied'); } catch(e){ addLog('error','Copy failed'); } });
        addLog('info','UI ready'); addLog('info','API '+API); addLog('info','Room '+roomId); addLog('info','Entity '+entityId);
        // Server logs SSE
        if (typeof LOGS_ENABLED !== 'undefined' && LOGS_ENABLED) {
          (function startSSE(){
            const urls = [API + '/logs', '/logs'];
            const idx = (window.__sseLogIdx||0);
            const url = urls[idx];
            try {
              const es = new EventSource(url);
              es.onopen = ()=>{ addLog('info','SSE connected '+url); };
              es.onmessage = (ev)=>{
                try{ const data = JSON.parse(ev.data); const level = (data.level||'info'); const msg = '['+(data.target||'')+'] '+(data.message||''); addLog(level, msg); }
                catch(e){ addLog('error','Log parse failed: '+(e && e.message ? e.message : String(e))); }
              };
              es.onerror = (ev)=>{
                const states = ['CONNECTING','OPEN','CLOSED'];
                const st = states[es.readyState] || String(es.readyState);
                addLog('error','SSE error ('+st+') '+(ev && ev.type ? ev.type : ''));
                try{ es.close(); } catch{}
                window.__sseLogIdx = (idx+1) % urls.length;
                setTimeout(startSSE, 2000);
              };
              setTimeout(()=>{ try{ es.close(); }catch{}; }, 60000);
            } catch(e) { addLog('error','SSE init failed: '+(e && e.message ? e.message : String(e))); }
          })();
        }

        async function loadCharacters() {
          try {
            const url = API.replace(/\/$/, '') + '/characters';
            const res = await fetchWithLog(url, undefined, 'characters');
            if (!res.ok) throw new Error('failed');
            const data = await res.json();
            const list = (data.characters || []);
            const current = (data.current || '').toLowerCase().replace(/[^a-z0-9]/g, '');
            charSelect.innerHTML = '';
            if (!list.length) {
              const opt = document.createElement('option');
              opt.value = ''; opt.textContent = 'No characters found'; charSelect.appendChild(opt);
              return;
            }
            let selectedIdx = 0;
            list.forEach((name, idx) => {
              const opt = document.createElement('option');
              opt.value = name; opt.textContent = name; charSelect.appendChild(opt);
              // Match current character by comparing normalized names
              const normalized = name.replace('.xml','').toLowerCase().replace(/[^a-z0-9]/g, '');
              if (current && (normalized.includes(current) || current.includes(normalized))) {
                selectedIdx = idx;
              }
            });
            charSelect.selectedIndex = selectedIdx;
          } catch (e) {
            // Fallback: show a message, avoid crashing UI
            charSelect.innerHTML = '';
            const opt = document.createElement('option');
            opt.value = ''; opt.textContent = 'API unavailable'; charSelect.appendChild(opt);
            addLog('error','Characters unavailable');
          }
        }
        applyChar.addEventListener('click', async () => {
          const filename = charSelect.value;
          if (!filename) return;
          await fetchWithLog(API.replace(/\/$/, '') + '/character/select', { method:'POST', headers:{'Content-Type':'application/json'}, body: JSON.stringify({ filename }) }, 'character.select');
          addLog('info','Character applied '+filename);
          // Refresh state panel
          await fetchState({ 'Content-Type': 'application/json' });
        });
        loadCharacters();

        function addUser(text) {
          const el = document.createElement('div');
          el.className = 'msg user';
          el.innerHTML = `<div class="bubble">${text}</div>`;
          chat.appendChild(el);
          chat.scrollTop = chat.scrollHeight;
        }

        function addAgent(text, thought) {
          if (thought) {
            const tEl = document.createElement('div');
            tEl.className = 'msg agent';
            const items = String(thought).split(/\r?\n/).map(s => s.replace(/^[-]\s*/, '').trim()).filter(Boolean);
            const paragraph = items.join(' ');
            tEl.innerHTML = `<div class="thoughts"><b>Internal Thought</b>: ${paragraph}</div>`;
            chat.appendChild(tEl);
          }
          const el = document.createElement('div');
          el.className = 'msg agent';
          el.innerHTML = `<div class="bubble">${text}</div>`;
          chat.appendChild(el);
          chat.scrollTop = chat.scrollHeight;
        }

        function typing(show) {
          let t = document.getElementById('typing');
          if (show && !t) {
            t = document.createElement('div');
            t.id = 'typing';
            t.className = 'msg agent';
            t.innerHTML = `<div class="bubble"><span class="typing"><span></span><span></span><span></span></span></div>`;
            chat.appendChild(t);
          } else if (!show && t) {
            t.remove();
          }
          chat.scrollTop = chat.scrollHeight;
        }

        function extractThought(text) {
          const m1 = text.match(/<thought>([\s\S]*?)<\/thought>/i);
          if (m1) return m1[1].trim();
          const m2 = text.match(/\b[Tt]houghts?:\s*([^\n]+)/);
          if (m2) return m2[1].trim();
          return null;
        }

        function parseReplyAndThought(text) {
          const src = String(text || '');
          const fence = src.match(/```(?:xml|reply|thought)?([\s\S]*?)```/i);
          let raw = fence ? fence[1] : src;
          let reply = null;
          let thought = null;

          // <text>...</text> common wrapper
          const textTag = raw.match(/<text>([\s\S]*?)<\/text>/i);
          if (textTag) reply = textTag[1].trim();

          // <reply>...</reply> multiline block
          const replyTag = raw.match(/<reply>([\s\S]*?)<\/reply>/i);
          if (replyTag) reply = replyTag[1].trim();

          // REPLY marker until next thought marker or end
          if (!reply) {
            const r = raw.match(/\bREPLY\b[\s\S]*?(.*?)(?=\n\s*(?:Thoughts?|THOUGHTS?|Chain[- ]?of[- ]?thought|Reasoning|COT)\b|$)/i);
            if (r) reply = r[1].trim();
          }

          // Thought markers (prefer explicit)
          const thoughtTag = raw.match(/<thought>([\s\S]*?)<\/thought>/i);
          if (thoughtTag) thought = thoughtTag[1].trim();
          if (!thought) {
            const t1 = raw.match(/\b(T[hH]oughts?|COT|Chain[- ]?of[- ]?thought|Reasoning)\b[:\-]?\s*([\s\S]*?)(?=```|<\/reply>|$)/);
            if (t1) thought = t1[2].trim();
          }

          // Fallbacks
          if (!reply) reply = src.trim();
          if (!thought) thought = extractThought(src);
          return { reply, thought };
        }

        function splitThoughtSteps(thought) {
          if (!thought) return [];
          const t = thought.trim();
          const lines = t.split(/\r?\n/).map(s => s.trim()).filter(Boolean);
          if (lines.length > 1) return lines;
          // Try bullet/numbered split in a single line
          const bullets = t.split(/\s*[-•]\s+/).map(s => s.trim()).filter(Boolean);
          if (bullets.length > 1) return bullets;
          const numbered = t.split(/\s*\d+\.?\s+/).map(s => s.trim()).filter(Boolean);
          if (numbered.length > 1) return numbered;
          return [t];
        }

        function scoreConfidence(text) {
          const t = String(text || '').toLowerCase();
          const hedges = ['maybe','might','perhaps','possibly','likely','seems','apparently'];
          let hits = 0; hedges.forEach(h=>{ if(t.includes(h)) hits++; });
          if (hits <= 1 && t.length > 80) return 'High';
          if (hits <= 2) return 'Medium';
          return 'Low';
        }

        function composeReflection(userText, replyText, thought, state) {
          const topics = (userText || '').toLowerCase().split(/[^a-z0-9]+/).filter(w=>w.length>3);
          const uniqTopics = Array.from(new Set(topics)).slice(0,4).join(', ');
          const strategy = (replyText || '').toLowerCase().includes('example') ? 'example-led' : 'explanatory';
          const confidence = scoreConfidence(replyText || thought || '');
          const items = [];
          items.push('Intent: respond and resolve request');
          if (uniqTopics) items.push('Topics: ' + uniqTopics);
          items.push('Strategy: ' + strategy);
          items.push('Confidence: ' + confidence);
          if (thought && thought.length > 0) items.push('Reasoning: ' + thought.slice(0, 120));
          items.push('Follow-up: ask for missing constraints if needed');
          return items;
        }

        async function addContextHint(key, value) {
          try {
            const headers = { 'Content-Type': 'application/json' };
            await fetchWithLog(API + '/context/add', { method:'POST', headers, body: JSON.stringify({ room_id: roomId, key, value }) }, 'context.add');
          } catch {}
        }

        async function saveThoughtSteps(steps) {
          try {
            const headers = { 'Content-Type': 'application/json' };
            await fetchWithLog(API + '/context/save', { method:'POST', headers, body: JSON.stringify({ room_id: roomId, steps }) }, 'context.save');
          } catch {}
        }

        const thoughtGroups = [];
        function addGroup(title) {
          const id = uuid();
          const group = { id, title, items: [], expanded: false, committed: false };
          thoughtGroups.push(group);
          return group;
        }
        function addGroupItem(group, text) {
          group.items.push(text);
        }
        function toggleGroup(id) {
          const g = thoughtGroups.find(x => x.id === id);
          if (!g) return;
          g.expanded = !g.expanded;
          renderChain();
        }
        function renderChain() {
          const el = document.getElementById('chain');
          if (!el) return;
          el.innerHTML = '';
          if (thoughtGroups.length === 0) {
            el.textContent = 'No thoughts yet';
            return;
          }
          thoughtGroups.forEach((g, gi) => {
            const groupEl = document.createElement('div');
            const header = document.createElement('div');
            header.style.cssText = 'display:flex; align-items:center; gap:8px; cursor:pointer; padding:6px 8px; border:1px solid rgba(34,211,238,.25); border-radius:8px; background:rgba(34,211,238,.08);';
            const idx = document.createElement('div');
            idx.style.cssText = 'min-width:22px; height:22px; border-radius:50%; background:rgba(34,211,238,.15); border:1px solid rgba(34,211,238,.4); color:#67e8f9; display:flex; align-items:center; justify-content:center; font-size:12px;';
            idx.textContent = String(gi+1);
            const title = document.createElement('div');
            title.style.cssText = 'flex:1;';
            const preview = (g.title || '').slice(0, 80);
            title.textContent = `Prompt: ${preview}`;
            const chevron = document.createElement('div');
            chevron.style.cssText = 'color:#67e8f9; font-size:12px;';
            chevron.textContent = g.expanded ? '▾' : '▸';
            const useBtn = document.createElement('button');
            useBtn.id = `usectx_${g.id}`;
            useBtn.textContent = g.committed ? 'Remove from Context' : 'Add to Context';
            useBtn.disabled = false;
            useBtn.style.cssText = g.committed
              ? 'padding:6px 10px; border-radius:8px; border:0; background:#1f2937; color:#9ca3af; font-weight:600; cursor:pointer;'
              : 'padding:6px 10px; border-radius:8px; border:0; background:linear-gradient(90deg, #22d3ee, #10b981); color:#051018; font-weight:600; cursor:pointer;';
            useBtn.onclick = (e) => { e.stopPropagation(); toggleGroupContext(g.id); };
            header.appendChild(idx);
            header.appendChild(title);
            header.appendChild(chevron);
            header.appendChild(useBtn);
            header.onclick = () => toggleGroup(g.id);
            groupEl.appendChild(header);
            const body = document.createElement('div');
            body.style.cssText = 'margin-top:6px; padding-left:2px; display:flex; flex-direction:column; gap:6px;';
            if (g.expanded) {
              g.items.forEach((t, i) => {
                const item = document.createElement('div');
                item.innerHTML = `<div style="display:flex; gap:8px; align-items:flex-start;">
                  <div style="min-width:22px; height:22px; border-radius:50%; background:rgba(34,211,238,.15); border:1px solid rgba(34,211,238,.4); color:#67e8f9; display:flex; align-items:center; justify-content:center; font-size:12px;">${i+1}</div>
                  <div style="flex:1;">${t}</div>
                </div>`;
                body.appendChild(item);
              });
            }
            groupEl.appendChild(body);
            el.appendChild(groupEl);
          });
        }

        async function useGroupContext(id) {
          const g = thoughtGroups.find(x => x.id === id);
          if (!g) return;
          const steps = ['Prompt: ' + (g.title || '')].concat(g.items);
          if (steps.length) {
            await addContextHint('lastThought', steps[0]);
            await saveThoughtSteps(steps);
            g.committed = true;
            const btn = document.getElementById(`usectx_${id}`);
            if (btn) {
              btn.textContent = 'Remove from Context';
              btn.disabled = false;
              btn.style.background = '#1f2937';
              btn.style.color = '#9ca3af';
              btn.style.cursor = 'pointer';
            }
            renderChain();
          }
        }

        async function removeGroupContext(id) {
          const g = thoughtGroups.find(x => x.id === id);
          if (!g) return;
          try {
            const headers = { 'Content-Type': 'application/json' };
            await fetchWithLog(API + '/context/remove', { method:'POST', headers, body: JSON.stringify({ room_id: roomId, id }) }, 'context.remove');
          } catch {}
          g.committed = false;
          const btn = document.getElementById(`usectx_${id}`);
          if (btn) {
            btn.textContent = 'Add to Context';
            btn.disabled = false;
            btn.style.background = 'linear-gradient(90deg, #22d3ee, #10b981)';
            btn.style.color = '#051018';
            btn.style.cursor = 'pointer';
          }
          renderChain();
        }

        async function toggleGroupContext(id) {
          const g = thoughtGroups.find(x => x.id === id);
          if (!g) return;
          if (!g.committed) {
            await useGroupContext(id);
          } else {
            await removeGroupContext(id);
          }
        }

        function inferPlan(text) {
          const lower = String(text || '').toLowerCase();
          const isQuestion = /\?|\b(how|what|why|when|where|who)\b/.test(lower);
          const steps = isQuestion
            ? ['clarify intent','identify topics','retrieve knowledge','compose answer']
            : ['determine goal','identify topics','plan structure','generate answer'];
          return steps.join(' → ');
        }

        async function fetchState(headers) {
          try {
            const res = await fetchWithLog(API + '/state', { method:'POST', headers, body: JSON.stringify({ roomId }) }, 'state');
            const data = await res.json();
            if (data.success && data.state) {
              document.getElementById('state').textContent = 'Ready';
              // Adapt thought chain from real agent state
              const steps = summarizeState(data.state);
              steps.forEach(s => { thoughtsChain.push(s); });
              renderChain();
              addLog('info','State updated');
            }
          } catch {}
        }

        function summarizeState(state) {
          // Defensive parsing – state may be arbitrary JSON
          const s = state || {};
          const data = s.data || s;
          const steps = [];

          // Preferred tone
          const tone = data?.characterSettings?.preferredTone;
          if (typeof tone === 'string' && tone.length > 0) {
            steps.push(`Tone set: ${tone}`);
          }

          // Topics or entities
          const entities = data?.entities || data?.keyEntities || data?.topics;
          if (Array.isArray(entities) && entities.length > 0) {
            steps.push(`Entities detected (${entities.length})`);
          }

          // Memory recall
          const memories = data?.recentMemories || data?.memories;
          if (Array.isArray(memories)) {
            const count = memories.length;
            steps.push(`Memory recall: ${count}`);
          }

          // Context size
          const ctx = data?.context || data?.promptContext || s?.context;
          if (typeof ctx === 'string' && ctx.length > 0) {
            steps.push(`Context composed (${Math.min(ctx.length, 200)} chars)`);
          }

          // Intent/goal
          const intent = data?.intent || data?.goal || data?.task;
          if (typeof intent === 'string' && intent.length > 0) {
            steps.push(`Intent: ${intent.slice(0, 60)}${intent.length > 60 ? '…' : ''}`);
          }

          // Fallback if empty
          if (steps.length === 0) {
            steps.push('State composed');
          }
          return steps;
        }

        async function pollTaskAndRender(headers, taskId) {
          let tries = 0;
          while (tries < 60) {
            const tr = await fetchWithLog(API + '/task/' + taskId, { headers }, 'task');
            const td = await tr.json();
            if (td.status === 'completed' && td.result) {
              typing(false);
              const msgs = (td.result && td.result.messages) ? td.result.messages : [];
              for (const m of msgs) {
                const textRaw = (m.content && m.content.text) ? m.content.text : JSON.stringify(m);
                const pt = parseReplyAndThought(textRaw);
                const replyText = pt.reply || textRaw;
                addAgent(replyText, pt.thought || null);
              }
              return true;
            } else if (td.status === 'failed') {
              typing(false);
              addAgent('Streaming error');
              return false;
            }
            await new Promise(r => setTimeout(r, 500));
            tries++;
          }
          typing(false);
          addAgent('Streaming error');
          return false;
        }

        async function sendMessage(text) {
          const headers = { 'Content-Type': 'application/json' };
          if (typeof TOKEN === 'string' && TOKEN) headers['Authorization'] = 'Bearer ' + TOKEN;
          const promptPlan = inferPlan(text);
          const group = addGroup(text);
          addGroupItem(group, 'Prompt plan: ' + promptPlan);
          renderChain();
          window.lastUserText = text;
          typing(true);
          const doStream = true;
          if (doStream) {
            try {
              const res = await fetchWithLog(API + '/chat/stream', { method:'POST', headers, body: JSON.stringify({ text, roomId, entityId, stream:true }) }, 'chat.stream');
              const reader = res.body.getReader();
              const decoder = new TextDecoder();
              let buffer = '';
              let assembled = '';
              let currentEvent = '';
              let closed = false;
              let streamError = false;
              let committed = false;
              let sawFinal = false;
              let lastChunkAt = Date.now();
              let firstChunkReceived = false;
              // No timeout before first chunk - Ollama can take minutes for large prompts
              // After first chunk arrives, 60s timeout between chunks
              let watchdog = setInterval(()=>{
                if (firstChunkReceived && !committed && (Date.now() - lastChunkAt) > 60000) {
                  streamError = true;
                }
              }, 1000);
              while (true) {
                const { value, done } = await reader.read();
                if (done) { closed = true; break; }
                buffer += decoder.decode(value, { stream: true });
                const lines = buffer.split('\n');
                buffer = lines.pop();
                for (const line of lines) {
                  if (line.startsWith('event:')) {
                    currentEvent = line.slice(6).trim();
                    if (currentEvent === 'error') { streamError = true; }
                  } else if (line.startsWith('data:')) {
                    try {
                      const payload = JSON.parse(line.slice(5));
                      if (payload.error) { streamError = true; }
                      const isFinal = payload.final || (currentEvent === 'complete');
                      if (streamError) { break; }
                      
                      if (isFinal && !committed) {
                        sawFinal = true;
                        typing(false);
                        const finalChunk = payload.text || '';
                        if (finalChunk) {
                          assembled += finalChunk;
                          const tnode = document.getElementById('typing');
                          if (tnode) { tnode.querySelector('.bubble').textContent = assembled; }
                        }
                        // Show whatever was assembled (don't send fallback requests)
                        const pt = parseReplyAndThought(assembled);
                        const replyText = pt.reply || assembled;
                        if (replyText && replyText.trim().length > 0) {
                          addAgent(replyText, pt.thought || null);
                        } else {
                          addAgent('Empty response. Please try again.');
                        }
                        committed = true;
                        try { clearInterval(watchdog); } catch {}
                      } else {
                        const chunk = payload.text || '';
                        if (chunk) {
                          assembled += chunk;
                          firstChunkReceived = true;
                          const tnode = document.getElementById('typing');
                          if (tnode) { tnode.querySelector('.bubble').textContent = assembled; }
                          lastChunkAt = Date.now();
                        }
                      }
                    } catch { streamError = true; }
                  }
                }
                if (streamError) { break; }
              }
              if (streamError && !committed) {
                // Don't send fallback requests - they can block the server
                typing(false);
                addAgent('Connection interrupted. Please try again.');
                committed = true;
                try { clearInterval(watchdog); } catch {}
              } else {
                if (closed && !committed) {
                  typing(false);
                  if (assembled && assembled.trim().length > 0) {
                    const pt = parseReplyAndThought(assembled);
                    addAgent(pt.reply || assembled, pt.thought || null);
                  } else {
                    addAgent('No response received. Please try again.');
                  }
                  committed = true;
                  try { clearInterval(watchdog); } catch {}
                }
              }
            } catch (e) {
              // Don't send fallback requests - they can block the server
              typing(false);
              addAgent('Request failed. Please try again.');
            }
          } else {
            const res = await fetchWithLog(API + '/chat', { method:'POST', headers, body: JSON.stringify({ text, roomId, entityId, stream:false }) }, 'chat');
            const data = await res.json();
            if (!data.success) {
              typing(false);
              addAgent('Error: ' + (data.error || 'unknown'));
              addLog('error','Chat error '+(data.error || 'unknown'));
              return;
            }
            const taskId = data.taskId;
            let tries = 0;
            while (tries < 60) {
              const tr = await fetchWithLog(API + '/task/' + taskId, { headers }, 'task');
              const td = await tr.json();
              if (td.status === 'completed' && td.result) {
                typing(false);
                const msgs = (td.result && td.result.messages) ? td.result.messages : [];
                addLog('info','Task completed with '+msgs.length+' messages');
                for (const m of msgs) {
                  const textRaw = (m.content && m.content.text) ? m.content.text : JSON.stringify(m);
                  const pt = parseReplyAndThought(textRaw);
                  const replyText = pt.reply || textRaw;
                  const thought = pt.thought || null;
                  addAgent(replyText, thought);
                }
                fetchState(headers);
                break;
              } else if (td.status === 'failed') {
                typing(false);
                addAgent('Task failed: ' + (td.error || 'unknown'));
                addLog('error','Task failed '+(td.error || 'unknown'));
                break;
              }
              await new Promise(r => setTimeout(r, 500));
              addLog('info','Polling task '+String(++tries));
            }
          }
        }

        btn.addEventListener('click', async () => {
          const text = input.value.trim();
          if (!text) return;
          addUser(text);
          input.value = '';
          await sendMessage(text);
        });
        input.addEventListener('keydown', async (e) => {
          if (e.key === 'Enter') { e.preventDefault(); btn.click(); }
        });
        
      </script>
    </body></html>"#;
    let html = template
        .replace("{API_URL}", api_url)
        .replace("{TOKEN_JS}", &token_js)
        .replace("{LOGS_JS}", &logs_js)
        .replace(
            "{USE_STREAMING}",
            if use_streaming { "true" } else { "false" },
        );
    Html(html)
}

async fn agent_proxy(
    AxumState(state): AxumState<SimpleUiServer>,
    Path(rest): Path<String>,
    req: Request,
) -> impl axum::response::IntoResponse {
    // Build destination URL
    let base = state.config.agent_api_url.trim_end_matches('/');
    let url = format!("{}/{}", base, rest);

    // Convert Axum Request to reqwest
    let method = match req.method().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        _ => reqwest::Method::GET,
    };
    let headers = req.headers().clone();
    let body_bytes = body::to_bytes(req.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap_or_default();

    let client = reqwest::Client::new();
    let mut rb = client.request(method, &url);
    // Copy headers (as strings)
    for (k, v) in headers.iter() {
        let k_str = k.as_str();
        if let Ok(v_str) = v.to_str() {
            rb = rb.header(k_str, v_str);
        }
    }
    // Send body
    let resp = rb.body(body_bytes).send().await;

    match resp {
        Ok(r) => {
            let status =
                StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let mut headers_out = axum::http::HeaderMap::new();
            for (k, v) in r.headers().iter() {
                let name_str = k.as_str();
                if [
                    "connection",
                    "keep-alive",
                    "proxy-authenticate",
                    "proxy-authorization",
                    "te",
                    "trailer",
                    "transfer-encoding",
                    "upgrade",
                    "content-length",
                ]
                .contains(&name_str.to_ascii_lowercase().as_str())
                {
                    continue;
                }
                if let Ok(name) = axum::http::HeaderName::from_bytes(name_str.as_bytes()) {
                    if let Ok(val_str) = v.to_str() {
                        if let Ok(val) = axum::http::HeaderValue::from_str(val_str) {
                            headers_out.insert(name, val);
                        }
                    }
                }
            }
            if rest.ends_with("chat/stream") {
                headers_out.insert(
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/event-stream"),
                );
                headers_out.insert(
                    axum::http::header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("no-cache, no-transform"),
                );
                headers_out.insert(
                    axum::http::HeaderName::from_static("x-accel-buffering"),
                    axum::http::HeaderValue::from_static("no"),
                );
            }
            let stream = r.bytes_stream();
            let body = Body::from_stream(stream);
            let mut resp_out = axum::response::Response::new(body);
            *resp_out.status_mut() = status;
            *resp_out.headers_mut() = headers_out;
            resp_out
        }
        Err(_) => {
            let mut resp_out = axum::response::Response::new(Body::from(""));
            *resp_out.status_mut() = StatusCode::BAD_GATEWAY;
            resp_out
        }
    }
}

fn scrub_message(mut s: String) -> String {
    if s.len() > 2000 {
        s = s.chars().take(2000).collect();
    }
    let patterns = [
        (Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap(), "sk-REDACTED"),
        (
            Regex::new(r"(?i)api[_-]?key\s*[:=]?\s*[A-Za-z0-9-_]{12,}").unwrap(),
            "api_key=REDACTED",
        ),
        (
            Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap(),
            "email@redacted",
        ),
        (
            Regex::new(r"\b\+?\d[\d\s-]{8,}\b").unwrap(),
            "PHONE_REDACTED",
        ),
    ];
    for (re, rep) in patterns.iter() {
        s = re.replace_all(&s, *rep).into_owned();
    }
    s
}

async fn ui_logs_sse() -> Sse<BoxStream<'static, std::result::Result<Event, Infallible>>> {
    let rx = subscribe_logs();
    let stream: BoxStream<'static, std::result::Result<Event, Infallible>> = match rx {
        Some(rx) => BroadcastStream::new(rx)
            .filter_map(|item| async move {
                match item {
                    Ok(mut ev) => {
                        ev.message = scrub_message(ev.message);
                        let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
                        Some(Ok(Event::default().data(data)))
                    }
                    Err(_) => None,
                }
            })
            .boxed(),
        None => BroadcastStream::new({
            let (tx, rx) = tokio::sync::broadcast::channel::<LogEvent>(1);
            let _ = tx.send(LogEvent {
                level: "INFO".into(),
                target: "init".into(),
                message: "logging not initialized".into(),
                file: None,
                line: None,
                time: chrono::Utc::now().to_rfc3339(),
            });
            rx
        })
        .filter_map(|item| async move {
            match item {
                Ok(mut ev) => {
                    ev.message = scrub_message(ev.message);
                    let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
                    Some(Ok(Event::default().data(data)))
                }
                Err(_) => None,
            }
        })
        .boxed(),
    };
    Sse::new(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[tokio::test]
    #[ignore]
    async fn simpleui_serves_index() {
        // bind an ephemeral port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // create dummy runtime
        let opts = zoey_core::RuntimeOpts::default();
        let runtime = zoey_core::AgentRuntime::new(opts).await.unwrap();

        // start UI
        let ui = SimpleUiServer::new(
            SimpleUiConfig {
                enabled: true,
                host: "127.0.0.1".to_string(),
                port,
                agent_api_url: format!("http://127.0.0.1:9090/agent"),
                use_streaming: false,
                token: None,
                logs_enabled: false,
            },
            runtime,
        );
        let _ = tokio::spawn(async move {
            let _ = ui.start().await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // fetch index
        let body = reqwest::get(format!("http://127.0.0.1:{}/", port))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(body.contains("Zoey Simple UI"));
        assert!(body.contains("TOKEN"));
    }
}
