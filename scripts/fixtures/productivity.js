// Local UI fixture: example snippets and state events, without authentication or native input.
(() => {
  let next = 1;
  const callbacks = new Map(), listeners = new Map();
  const snippet = (id, name, keyword, text, profile = "common") => ({ id, name, keyword, content: { PlainText: text }, profile, template: id === "template", enabled: true, trigger_mode: "Space" });
  const f = window.productivityFixture = {
    rows: [snippet("plain", "일반 문구", ":plain", "입력 완료"), snippet("template", "고객 안내", ":hello", "{{고객명}}님, {{날짜}}에 안내드려요"), { ...snippet("vault", "연결 계정", ":account", ""), content: { BitwardenRef: { item_id: "00000000-0000-4000-8000-000000000001", field: "Password" } } }],
    engine: { active: true, paused: false, vault: "Locked", error: null },
    product: { settings: { active_profile: "work", rules: [] }, effective_profile: "work", application: "example.exe", readiness: { plain: "ready", template: "template", vault: "vault_locked" } },
    context: { token: 42, snippet_id: null, application: "example.exe", profile: "work" },
    calls: [], submissions: [],
    emit(event, payload) { for (const { event: name, handler } of listeners.values()) if (name === event) callbacks.get(handler)?.({ event, payload }); },
    open(id = null) { this.context = { ...this.context, token: this.context.token + 1, snippet_id: id }; this.emit("quick-open", null); },
  };
  window.isTauri = true;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener(_event, id) { listeners.delete(id); } };
  window.__TAURI_INTERNALS__ = {
    transformCallback(fn) { const id = next++; callbacks.set(id, fn); return id; },
    unregisterCallback(id) { callbacks.delete(id); },
    async invoke(command, args) {
      f.calls.push(command);
      switch (command) {
        case "plugin:event|listen": { const id = next++; listeners.set(id, args); return id; }
        case "plugin:event|unlisten": listeners.delete(args.eventId); return;
        case "get_engine_status": return structuredClone(f.engine);
        case "get_productivity_state": return structuredClone(f.product);
        case "list_snippets": return structuredClone(f.rows);
        case "get_quick_context": return structuredClone(f.context);
        case "submit_quick": f.submissions.push(args); f.emit("quick-close", null); return;
        case "cancel_quick": f.emit("quick-close", null); return;
        case "diagnose_snippet": return [];
        case "save_profiles": f.product.settings = args.settings; f.product.effective_profile = args.settings.active_profile; f.emit("productivity-state", structuredClone(f.product)); return;
        case "set_engine_paused": f.engine.paused = args.paused; f.emit("engine-state", structuredClone(f.engine)); return;
        case "clear_buffer": return;
        case "get_vault_status": return { status: "Locked", cli_available: true, cached_fields: 0 };
        default: throw Error(`Unsupported fixture command: ${command}`);
      }
    },
  };
})();
