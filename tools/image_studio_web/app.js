'use strict';
(function () {
  var DRAFT_KEY = 'ai-image-studio-draft-v1';
  var POLL_MS = 2000;

  // ---------- 상태 (카탈로그/기본값/제한은 /api/config 응답만 사용) ----------
  var state = {
    csrfToken: null,
    models: [],
    modelsById: {},
    defaults: null,
    limits: null,
    outputDir: '',
    keyConfigured: false,
    configLoaded: false,
    refs: [],
    jobs: [],
    jobsById: {},
    selectedJobId: null,
    activeJobId: null,
    serverLost: false,
    lastAvailable: null,
    lastCheckOk: false,
    checking: false,
    submitting: false,
    shuttingDown: false
  };
  var pollTimer = null;
  var elapsedTimer = null;
  var pollFailCount = 0;
  var draftSaveTimer = null;

  // ---------- DOM ----------
  function $(id) { return document.getElementById(id); }
  var serverStatus = $('serverStatus');
  var serverStatusText = $('serverStatusText');
  var serverBanner = $('serverBanner');
  var serverBannerText = $('serverBannerText');
  var bannerRefreshBtn = $('bannerRefreshBtn');
  var connBaseUrl = $('connBaseUrl');
  var connApiKey = $('connApiKey');
  var connTestBtn = $('connTestBtn');
  var connStatus = $('connStatus');
  var connError = $('connError');
  var connAvailable = $('connAvailable');
  var keyHint = $('keyHint');
  var genForm = $('genForm');
  var modelSelect = $('modelSelect');
  var sizeSelect = $('sizeSelect');
  var qualitySelect = $('qualitySelect');
  var modelNote = $('modelNote');
  var sizeHelp = $('sizeHelp');
  var qualityNote = $('qualityNote');
  var promptInput = $('prompt');
  var positiveInput = $('positive');
  var negativeInput = $('negative');
  var dropZone = $('dropZone');
  var refInput = $('refInput');
  var refPreviews = $('refPreviews');
  var refHint = $('refHint');
  var refWarning = $('refWarning');
  var refLimitLabel = $('refLimitLabel');
  var generateBtn = $('generateBtn');
  var formStatus = $('formStatus');
  var formError = $('formError');
  var curStatus = $('curStatus');
  var curElapsed = $('curElapsed');
  var curMeta = $('curMeta');
  var curError = $('curError');
  var resultImages = $('resultImages');
  var resultEmpty = $('resultEmpty');
  var curPrompts = $('curPrompts');
  var curPromptText = $('curPromptText');
  var curPositiveText = $('curPositiveText');
  var curNegativeText = $('curNegativeText');
  var reuseBtn = $('reuseBtn');
  var outputDirEl = $('outputDir');
  var historyList = $('historyList');
  var historyEmpty = $('historyEmpty');
  var refreshJobsBtn = $('refreshJobsBtn');
  var shutdownBtn = $('shutdownBtn');
  var shutdownStatus = $('shutdownStatus');
  var connDetails = $('connDetails');
  var connSummaryStatus = $('connSummaryStatus');

  function setText(node, text) {
    node.textContent = text == null ? '' : String(text);
  }
  function show(node) { node.hidden = false; }
  function hide(node) { node.hidden = true; }
  function fmtBytes(n) {
    if (n >= 1048576) return (n / 1048576).toFixed(1) + 'MB';
    if (n >= 1024) return (n / 1024).toFixed(1) + 'KB';
    return n + 'B';
  }
  function fmtTime(iso) {
    if (!iso) return '';
    var d = new Date(iso);
    if (isNaN(d.getTime())) return String(iso);
    return d.toLocaleString('ko-KR');
  }
  function jobCreatedMs(job) {
    if (!job || job.created_at == null) return NaN;
    var v = job.created_at;
    if (typeof v === 'number') return v < 1e12 ? v * 1000 : v;
    var t = Date.parse(v);
    return t;
  }
  function statusLabel(s) {
    if (s === 'queued') return '대기 중';
    if (s === 'running') return '생성 중';
    if (s === 'succeeded') return '완료';
    if (s === 'failed') return '실패';
    return s || '알 수 없음';
  }

  // ---------- 서버 상태 ----------
  function setServerState(kind, text) {
    serverStatus.setAttribute('data-state', kind);
    setText(serverStatusText, text);
  }
  function showBanner(text) {
    setText(serverBannerText, text);
    show(serverBanner);
  }
  function hideBanner() { hide(serverBanner); }
  function markServerLost(message) {
    state.serverLost = true;
    setServerState('bad', '연결 끊김');
    showBanner(message || '서버와 연결이 끊겼습니다. 서버 콘솔을 확인한 뒤 페이지를 새로고침하세요. 자동으로 다시 생성하지 않습니다.');
    updateGenerateState();
  }
  function setConnDetails(open, summary) {
    if (connDetails) connDetails.open = !!open;
    if (connSummaryStatus) setText(connSummaryStatus, summary || '');
  }

  // ---------- fetch ----------
  function apiGet(path) {
    return fetch(path, { method: 'GET', credentials: 'same-origin' }).then(function (res) {
      if (!res.ok) throw new Error('HTTP ' + res.status);
      return res.json();
    });
  }
  function apiPost(path, body) {
    if (!state.csrfToken) return Promise.reject(new Error('설정을 먼저 불러오세요 (새로고침 후 다시 시도).'));
    return fetch(path, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json', 'X-Studio-Token': state.csrfToken },
      body: JSON.stringify(body)
    }).then(function (res) {
      return res.json().then(function (data) {
        return { status: res.status, ok: res.ok, data: data };
      }).catch(function () {
        throw new Error('서버 응답 해석 실패 (HTTP ' + res.status + ')');
      });
    });
  }

  // ---------- 초안 (텍스트/모델 설정만, 키 제외) ----------
  function loadDraft() {
    try {
      var raw = localStorage.getItem(DRAFT_KEY);
      if (!raw) return null;
      var d = JSON.parse(raw);
      if (!d || typeof d !== 'object') return null;
      return d;
    } catch (e) { return null; }
  }
  function saveDraft() {
    if (!state.configLoaded) return;
    try {
      var d = {
        prompt: promptInput.value,
        positive: positiveInput.value,
        negative: negativeInput.value,
        image_model: modelSelect.value,
        size: sizeSelect.value,
        quality: qualitySelect.value
      };
      localStorage.setItem(DRAFT_KEY, JSON.stringify(d));
    } catch (e) { /* 저장 실패는 무시 */ }
  }
  function scheduleDraftSave() {
    clearTimeout(draftSaveTimer);
    draftSaveTimer = setTimeout(saveDraft, 250);
  }
  function modelSizes(m) {
    if (!m || !Array.isArray(m.sizes)) return [];
    return m.sizes.map(function (s) {
      return (s && typeof s === 'object') ? s.value : s;
    }).filter(Boolean);
  }
  function modelSizeLabel(m, v) {
    if (m && Array.isArray(m.sizes)) {
      for (var i = 0; i < m.sizes.length; i++) {
        var s = m.sizes[i];
        if (s && typeof s === 'object' && s.value === v) return s.label || v;
      }
    }
    return v;
  }
  function modelQualities(m) {
    if (m && Array.isArray(m.qualities)) return m.qualities.slice();
    return [];
  }
  function fillSelect(sel, values, current, labelFn) {
    while (sel.firstChild) sel.removeChild(sel.firstChild);
    values.forEach(function (v) {
      var opt = document.createElement('option');
      opt.value = v;
      setText(opt, labelFn ? labelFn(v) : v);
      sel.appendChild(opt);
    });
    if (values.indexOf(current) >= 0) sel.value = current;
  }
  function renderModelOptions(keep) {
    while (modelSelect.firstChild) modelSelect.removeChild(modelSelect.firstChild);
    state.models.forEach(function (m) {
      var opt = document.createElement('option');
      opt.value = m.id;
      var label = m.label || m.id;
      if (m.provider) label += ' · ' + m.provider;
      setText(opt, label);
      modelSelect.appendChild(opt);
    });
    var has = state.models.length > 0;
    modelSelect.disabled = !has;
    sizeSelect.disabled = !has;
    qualitySelect.disabled = !has;
    if (!has) return;
    var defModel = state.defaults && state.defaults.image_model;
    if (keep && state.modelsById[keep]) modelSelect.value = keep;
    else if (defModel && state.modelsById[defModel]) modelSelect.value = defModel;
    else modelSelect.value = state.models[0].id;
  }
  function renderSizeQuality(keepSize, keepQuality) {
    var m = state.modelsById[modelSelect.value];
    var sizes = modelSizes(m);
    var quals = modelQualities(m);
    if (!sizes.length || !quals.length) return;
    var defSize = state.defaults && state.defaults.size;
    var defQuality = state.defaults && state.defaults.quality;
    var sizeCur = (keepSize && sizes.indexOf(keepSize) >= 0) ? keepSize : sizes[0];
    if (!keepSize && sizeSelect.value && sizes.indexOf(sizeSelect.value) >= 0) sizeCur = sizeSelect.value;
    else if (!keepSize && defSize && sizes.indexOf(defSize) >= 0) sizeCur = defSize;
    var qualCur = (keepQuality && quals.indexOf(keepQuality) >= 0) ? keepQuality : quals[0];
    if (!keepQuality && qualitySelect.value && quals.indexOf(qualitySelect.value) >= 0) qualCur = qualitySelect.value;
    else if (!keepQuality && defQuality && quals.indexOf(defQuality) >= 0) qualCur = defQuality;
    fillSelect(sizeSelect, sizes, sizeCur, function (v) { return modelSizeLabel(m, v); });
    fillSelect(qualitySelect, quals, qualCur, function (v) { return v === 'high' ? '고품질' : v === 'medium' ? '표준' : '낮음'; });
  }
  function renderModelNote() {
    var m = state.modelsById[modelSelect.value];
    setText(modelNote, m ? (m.reference_images ? '참조 이미지 사용 가능' : '텍스트 전용 · 참조 이미지 사용 불가') : '');
    modelNote.title = m ? m.note : '';
  }
  function renderLimits() {
    if (!state.limits) {
      setText(refLimitLabel, '(확인 중)');
      setText(refHint, '모델을 바꿔도 첨부된 참조는 절대 자동 삭제하지 않습니다.');
      return;
    }
    var c = state.limits.reference_count;
    var b = state.limits.reference_bytes;
    setText(refLimitLabel, '(최대 ' + c + '장 · 장당 ' + fmtBytes(b) + ')');
    setText(refHint, 'Grok은 참조 이미지를 지원하지 않습니다. 모델을 바꿔도 첨부한 파일은 유지됩니다.');
  }
  function renderAvailable() {
    while (connAvailable.firstChild) connAvailable.removeChild(connAvailable.firstChild);
    state.models.forEach(function (m) {
      var li = document.createElement('li');
      var name = document.createElement('span');
      setText(name, (m.label || m.id) + ' (' + m.id + ')');
      var st = document.createElement('span');
      if (state.lastAvailable === null) {
        setText(st, '미확인');
        st.className = 'st-unknown';
      } else if (state.lastAvailable.indexOf(m.id) >= 0) {
        setText(st, '사용 가능');
        st.className = 'st-ok';
      } else {
        setText(st, '목록에 없음');
        st.className = 'st-no';
      }
      li.appendChild(name);
      li.appendChild(st);
      connAvailable.appendChild(li);
    });
  }

  // ---------- 참조 이미지 ----------
  function updateRefWarning() {
    var m = state.modelsById[modelSelect.value];
    if (state.refs.length > 0 && m && m.reference_images === false) {
      setText(refWarning, '선택한 모델(' + m.id + ')은 참조 이미지를 지원하지 않습니다. 생성을 계속하려면 참조를 모두 제거하거나, 참조 가능한 모델(gpt-image-2.5 계열, gpt-image-2, gpt-image-1.5, gemini-3.1-flash-image)로 바꾸세요. 첨부 파일은 그대로 보관되며 자동 삭제하지 않습니다.');
      show(refWarning);
      return true;
    }
    hide(refWarning);
    return false;
  }
  function renderRefs() {
    while (refPreviews.firstChild) refPreviews.removeChild(refPreviews.firstChild);
    state.refs.forEach(function (r, idx) {
      var li = document.createElement('li');
      var img = document.createElement('img');
      img.src = r.data_url;
      img.alt = '참조 이미지: ' + r.name;
      var meta = document.createElement('div');
      meta.className = 'ref-meta';
      var p = document.createElement('p');
      setText(p, r.name + ' · ' + fmtBytes(r.bytes));
      p.title = r.name;
      var btn = document.createElement('button');
      btn.type = 'button';
      setText(btn, '제거');
      btn.setAttribute('aria-label', '참조 이미지 제거: ' + r.name);
      btn.addEventListener('click', function () {
        state.refs.splice(idx, 1);
        renderRefs();
        updateRefWarning();
        updateGenerateState();
      });
      meta.appendChild(p);
      meta.appendChild(btn);
      li.appendChild(img);
      li.appendChild(meta);
      refPreviews.appendChild(li);
    });
    updateRefWarning();
  }
  function addFiles(files) {
    hide(formError);
    if (!state.configLoaded || !state.limits) {
      showFormError('설정을 불러오는 중입니다. 잠시 후 다시 시도하세요.');
      return;
    }
    var c = state.limits.reference_count;
    var b = state.limits.reference_bytes;
    var list = Array.prototype.slice.call(files || []);
    if (!list.length) return;
    var room = c - state.refs.length;
    if (room <= 0) {
      showFormError('참조 이미지는 최대 ' + c + '장까지 첨부할 수 있습니다. 먼저 기존 항목을 제거하세요. (자동 삭제하지 않음)');
      return;
    }
    if (list.length > room) {
      showFormError('한 번에 ' + room + '장까지만 추가할 수 있습니다 (최대 ' + c + '장). 초과분은 추가하지 않았습니다.');
      list = list.slice(0, room);
    }
    var pending = list.filter(function (f) {
      if (f.size > b) {
        showFormError('"' + f.name + '" (' + fmtBytes(f.size) + ')는 장당 상한 ' + fmtBytes(b) + '을 초과합니다. 추가하지 않았습니다.');
        return false;
      }
      if (f.type && f.type.indexOf('image/') !== 0) {
        showFormError('"' + f.name + '"는 이미지 파일이 아닙니다. 이미지 파일만 첨부할 수 있습니다.');
        return false;
      }
      return true;
    });
    pending.forEach(function (f) {
      var reader = new FileReader();
      reader.onload = function () {
        var url = String(reader.result || '');
        if (url.indexOf('data:image/') !== 0) {
          showFormError('"' + f.name + '"를 읽을 수 없습니다. 다른 이미지로 시도하세요.');
          return;
        }
        state.refs.push({ name: f.name || 'reference.png', data_url: url, bytes: f.size });
        renderRefs();
        updateGenerateState();
      };
      reader.onerror = function () {
        showFormError('"' + f.name + '"를 읽는 중 오류가 발생했습니다.');
      };
      reader.readAsDataURL(f);
    });
  }

  // ---------- 폼 상태/에러 ----------
  function showFormError(msg) {
    setText(formError, msg);
    show(formError);
    try { formError.focus({ preventScroll: false }); } catch (e) { try { formError.focus(); } catch (e2) {} }
  }
  function setFormStatus(msg, isBad) {
    setText(formStatus, msg);
    formStatus.style.color = isBad ? 'var(--danger)' : '';
  }
  function grokBlocked() {
    var m = state.modelsById[modelSelect.value];
    return !!(state.refs.length > 0 && m && m.reference_images === false);
  }
  function availabilityBlocked() {
    if (!state.lastCheckOk) return '연결 확인을 먼저 성공시키세요. [연결 설정]에서 [연결 확인]을 실행하세요.';
    if (state.lastAvailable !== null && state.lastAvailable.indexOf(modelSelect.value) < 0) {
      return '연결 확인 결과에 선택한 모델이 없습니다 (' + modelSelect.value + '). [연결 확인]을 다시 실행해 사용 가능 상태를 갱신한 뒤 생성하세요.';
    }
    return null;
  }
  function updateGenerateState() {
    updateRefWarning();
    var busy = !!state.activeJobId || !!state.submitting;
    if (shutdownBtn) shutdownBtn.disabled = busy || state.serverLost || !!state.shuttingDown || !state.configLoaded;
    if (!state.configLoaded || !state.lastCheckOk || state.serverLost || state.shuttingDown || state.submitting || state.activeJobId || state.checking) {
      generateBtn.disabled = true;
      setText(generateBtn, busy ? '생성 중…' : '한 장 생성');
      return;
    }
    if (grokBlocked() || availabilityBlocked()) {
      generateBtn.disabled = true;
      setText(generateBtn, '한 장 생성');
      return;
    }
    generateBtn.disabled = false;
    setText(generateBtn, '한 장 생성');
  }

  // ---------- 설정/작업 로드 ----------
  function fetchConfig() {
    return apiGet('/api/config').then(function (cfg) {
      state.csrfToken = cfg.csrf_token || null;
      if (Array.isArray(cfg.models)) state.models = cfg.models;
      state.modelsById = Object.fromEntries(state.models.map(function (model) { return [model.id, model]; }));
      if (cfg.defaults) state.defaults = cfg.defaults;
      if (cfg.limits) state.limits = cfg.limits;
      if (typeof cfg.base_url === 'string' && cfg.base_url && !connBaseUrl.value) {
        connBaseUrl.value = cfg.base_url;
      }
      state.keyConfigured = !!cfg.key_configured;
      setKeyHint();
      state.outputDir = (typeof cfg.output_dir === 'string') ? cfg.output_dir : '';
      setText(outputDirEl, state.outputDir || '(서버 응답에 저장 위치 없음)');
      state.configLoaded = true;
      // 초안 적용 (카탈로그 검증 후)
      var draft = loadDraft();
      var keepModel = state.defaults && state.defaults.image_model;
      var keepSize = state.defaults && state.defaults.size;
      var keepQuality = state.defaults && state.defaults.quality;
      if (draft) {
        if (draft.image_model && state.modelsById[draft.image_model]) keepModel = draft.image_model;
        if (typeof draft.prompt === 'string') promptInput.value = draft.prompt;
        if (typeof draft.positive === 'string') positiveInput.value = draft.positive;
        if (typeof draft.negative === 'string') negativeInput.value = draft.negative;
        keepSize = draft.size || keepSize;
        keepQuality = draft.quality || keepQuality;
      }
      renderModelOptions(keepModel);
      renderSizeQuality(keepSize, keepQuality);
      renderModelNote();
      renderLimits();
      renderAvailable();
      updateGenerateState();
      setServerState('ok', '서버 연결됨');
      if (state.keyConfigured) {
        // 키가 서버 세션에 있으면 설정란을 접고 생성 확인만 자동 실행 (생성 없음)
        setConnDetails(false, '· 연결 확인 중…');
        testConnection({ auto: true });
      } else {
        setConnDetails(true, '· 키 입력 필요');
      }
    });
  }
  function fetchHealth() {
    return apiGet('/api/health').then(function (h) {
      if (h && h.app === 'ai-image-studio') {
        if (!state.serverLost) setServerState('ok', '서버 연결됨');
        return true;
      }
      markServerLost('같은 포트에 다른 프로그램이 응답하고 있습니다 (ai-image-studio가 아님). 런처/포트를 확인하고 새로고침하세요.');
      return false;
    }).catch(function () {
      markServerLost();
      return false;
    });
  }
  function fetchJobs() {
    return apiGet('/api/jobs').then(function (data) {
      var arr = Array.isArray(data) ? data : data.jobs;
      if (!Array.isArray(arr)) arr = [];
      arr.sort(function (a, b) {
        var ta = jobCreatedMs(a);
        var tb = jobCreatedMs(b);
        if (isNaN(ta) && isNaN(tb)) return 0;
        if (isNaN(ta)) return 1;
        if (isNaN(tb)) return -1;
        return tb - ta;
      });
      state.jobs = arr;
      state.jobsById = {};
      arr.forEach(function (j) { if (j && j.id) state.jobsById[j.id] = j; });
      // 활성 작업 추적 (서버가 newest-first로 주므로 최신 active 우선)
      if (!state.activeJobId) {
        for (var i = 0; i < arr.length; i++) {
          if (arr[i].status === 'queued' || arr[i].status === 'running') {
            state.activeJobId = arr[i].id;
            startPolling();
            setFormStatus('이전 생성 진행을 이어서 확인합니다 (자동 재생성 아님).');
            break;
          }
        }
      }
      if (!state.selectedJobId && arr.length) state.selectedJobId = arr[0].id;
      if (state.selectedJobId && !state.jobsById[state.selectedJobId] && arr.length) {
        state.selectedJobId = arr[0].id;
      }
      renderJobs();
      renderCurrent();
      updateGenerateState();
    });
  }
  function fetchJob(id) {
    return apiGet('/api/jobs/' + encodeURIComponent(id)).then(function (job) {
      if (job && job.id) {
        state.jobsById[job.id] = job;
        var idx = -1;
        for (var i = 0; i < state.jobs.length; i++) {
          if (state.jobs[i].id === job.id) { idx = i; break; }
        }
        if (idx >= 0) state.jobs[idx] = job;
        else { state.jobs.unshift(job); }
      }
      return job;
    });
  }

  // ---------- 렌더: 기록/현재 ----------
  function renderJobs() {
    while (historyList.firstChild) historyList.removeChild(historyList.firstChild);
    if (!state.jobs.length) {
      show(historyEmpty);
      return;
    }
    hide(historyEmpty);
    state.jobs.forEach(function (job) {
      var li = document.createElement('li');
      var btn = document.createElement('button');
      btn.type = 'button';
      if (job.id === state.selectedJobId) btn.setAttribute('aria-current', 'true');
      var top = document.createElement('span');
      top.className = 'hist-top';
      var idSpan = document.createElement('span');
      setText(idSpan, fmtTime(job.created_at));
      var st = document.createElement('span');
      st.className = 'st st-' + (job.status || 'unknown');
      setText(st, statusLabel(job.status));
      top.appendChild(idSpan);
      top.appendChild(st);
      var mid = document.createElement('span');
      mid.className = 'hist-model';
      var imgCount = Array.isArray(job.images) ? job.images.length : 0;
      setText(mid, (job.image_model || '') + ' · ' + (job.size || '') + ' · ' + (job.quality || '') + (job.error ? ' · 오류 있음' : '') + (imgCount ? ' · 이미지 ' + imgCount : ''));
      var pr = document.createElement('span');
      pr.className = 'hist-prompt';
      setText(pr, job.prompt || job.positive || '(프롬프트 없음)');
      btn.appendChild(top);
      btn.appendChild(mid);
      btn.appendChild(pr);
      btn.addEventListener('click', function () {
        state.selectedJobId = job.id;
        renderJobs();
        renderCurrent();
        var sel = state.jobsById[job.id];
        if (sel && (sel.status === 'queued' || sel.status === 'running')) {
          state.activeJobId = sel.id;
          startPolling();
        }
        updateGenerateState();
      });
      li.appendChild(btn);
      historyList.appendChild(li);
    });
  }
  function clearImages() {
    while (resultImages.firstChild) resultImages.removeChild(resultImages.firstChild);
  }
  function renderCurrent() {
    var job = state.selectedJobId ? state.jobsById[state.selectedJobId] : null;
    if (!job) {
      setText(curStatus, '대기');
      curStatus.setAttribute('data-status', '');
      setText(curElapsed, '');
      setText(curMeta, '');
      hide(curError);
      clearImages();
      show(resultEmpty);
      hide(curPrompts);
      return;
    }
    hide(resultEmpty);
    setText(curStatus, statusLabel(job.status));
    curStatus.setAttribute('data-status', job.status || '');
    var metaParts = [];
    if (job.image_model) metaParts.push(job.image_model);
    if (job.size) metaParts.push('최종 ' + job.size);
    if (job.quality) metaParts.push(job.quality);
    if (typeof job.reference_count === 'number') metaParts.push('참조 ' + job.reference_count + '장');
    setText(curMeta, metaParts.join(' · '));
    if (job.error) {
      var errText = typeof job.error === 'string' ? job.error : String(job.error);
      if (Array.isArray(job.images) && job.images.length) {
        setText(curError, '일부 오류: ' + errText + ' (성공한 이미지는 아래에 그대로 표시됩니다)');
      } else {
        setText(curError, errText);
      }
      show(curError);
    } else {
      hide(curError);
    }
    // 이미지 (부분 성공 보존: 있는 것은 모두 표시)
    clearImages();
    var imgs = Array.isArray(job.images) ? job.images : [];
    if (!imgs.length) {
      if (job.status === 'succeeded') {
        show(resultEmpty);
        resultEmpty.firstElementChild.textContent = '작업은 완료되었지만 표시할 이미지가 없습니다. 저장 위치를 확인하세요.';
      } else if (job.status === 'failed') {
        show(resultEmpty);
        resultEmpty.firstElementChild.textContent = '실패한 작업입니다. 오류 메시지를 확인하세요.';
      } else {
        show(resultEmpty);
        resultEmpty.firstElementChild.textContent = '생성이 진행 중입니다. 잠시 후 자동으로 갱신됩니다.';
      }
    } else {
      hide(resultEmpty);
      imgs.forEach(function (im, i) {
        if (!im || typeof im.url !== 'string' || !im.url.startsWith('/files/')) return;
        var fig = document.createElement('figure');
        var img = document.createElement('img');
        img.src = im.url;
        img.alt = '생성 결과 ' + (i + 1) + (im.name ? ': ' + im.name : '');
        img.loading = 'lazy';
        var cap = document.createElement('figcaption');
        var info = document.createElement('span');
        var dims = (im.width && im.height) ? im.width + '×' + im.height + ' (최종)' : '최종 크기: ' + (job.size || '확인 중');
        setText(info, (im.name || 'image ' + (i + 1)) + ' · ' + dims);
        var links = document.createElement('span');
        links.className = 'img-links';
        var view = document.createElement('a');
        view.href = im.url;
        view.target = '_blank';
        view.rel = 'noopener';
        setText(view, '원본 보기');
        var dl = document.createElement('a');
        dl.href = im.url;
        dl.setAttribute('download', im.name || ('studio-' + String(job.id).slice(0, 8) + '-' + (i + 1) + '.png'));
        setText(dl, '다운로드');
        links.appendChild(view);
        links.appendChild(dl);
        cap.appendChild(info);
        cap.appendChild(links);
        fig.appendChild(img);
        fig.appendChild(cap);
        resultImages.appendChild(fig);
      });
    }
    // 프롬프트 표시
    show(curPrompts);
    setText(curPromptText, job.prompt || '(없음)');
    setText(curPositiveText, job.positive || '(없음)');
    setText(curNegativeText, job.negative || '(없음)');
    updateElapsedLine(job);
  }
  function updateElapsedLine(job) {
    job = job || (state.selectedJobId ? state.jobsById[state.selectedJobId] : null);
    if (!job) { setText(curElapsed, ''); return; }
    if (job.status === 'succeeded' || job.status === 'failed') {
      if (typeof job.elapsed_sec === 'number') setText(curElapsed, '소요 ' + job.elapsed_sec + '초 · ' + statusLabel(job.status));
      else setText(curElapsed, statusLabel(job.status));
      return;
    }
    var ms = jobCreatedMs(job);
    if (!isNaN(ms)) {
      var s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
      setText(curElapsed, '생성 중 · ' + s + '초 경과');
    } else if (typeof job.elapsed_sec === 'number') {
      setText(curElapsed, '경과 약 ' + job.elapsed_sec + '초 · ' + statusLabel(job.status));
    } else {
      setText(curElapsed, statusLabel(job.status) + ' — 상태를 확인하고 있습니다.');
    }
  }

  // ---------- 폴링 ----------
  function startPolling() {
    stopPolling(false);
    pollFailCount = 0;
    if (!state.activeJobId) return;
    if (!elapsedTimer) {
      elapsedTimer = setInterval(function () { updateElapsedLine(); }, 1000);
    }
    pollTimer = setInterval(pollOnce, POLL_MS);
    pollOnce();
  }
  function stopPolling(stopElapsed) {
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
    if (stopElapsed !== false && elapsedTimer && !state.activeJobId) {
      clearInterval(elapsedTimer);
      elapsedTimer = null;
    }
  }
  function pollOnce() {
    var id = state.activeJobId;
    if (!id) { stopPolling(); return; }
    fetchJob(id).then(function (job) {
      pollFailCount = 0;
      if (!job) return;
      renderJobs();
      if (state.selectedJobId === id) renderCurrent();
      if (job.status === 'succeeded' || job.status === 'failed') {
        var doneId = id;
        state.activeJobId = null;
        stopPolling();
        fetchJobs().then(function () {
          state.selectedJobId = doneId;
          renderJobs();
          renderCurrent();
          if (job.status === 'succeeded') setFormStatus('생성이 완료되었습니다.');
          else setFormStatus('생성이 실패했습니다. 현재 결과 카드의 오류를 확인하세요.', true);
          updateGenerateState();
        });
      } else {
        setFormStatus('생성 중입니다… (경과 시간은 오른쪽 결과 카드 참조)');
      }
      updateGenerateState();
    }).catch(function () {
      pollFailCount++;
      if (pollFailCount >= 3) {
        state.activeJobId = null;
        stopPolling();
        setFormStatus('서버 연결이 끊겨 상태 확인을 중단했습니다. [목록 새로고침] 후 확인하세요. 자동 재생성하지 않습니다.', true);
        markServerLost('서버와 연결이 끊겼습니다. 생성 요청을 자동으로 다시 보내지 않습니다. 서버 콘솔을 확인하고 새로고침하세요.');
      }
    });
  }

  // ---------- 연결 확인 ----------
  function setKeyHint() {
    setText(keyHint, state.keyConfigured
      ? '서버 세션에 키가 보관되어 있습니다. 새로고침해도 유지되며, 비워 두면 기존 키를 유지합니다.'
      : '서버에 보관된 키가 없습니다. API 키를 입력하고 [연결 확인]을 실행하세요.');
  }
  function testConnection(arg) {
    var auto = !!(arg && arg.auto);
    if (state.checking) return;
    state.checking = true;
    connTestBtn.disabled = true;
    hide(connError);
    setText(connStatus, '확인 중…');
    connStatus.className = 'conn-status';
    updateGenerateState();
    var body = { base_url: connBaseUrl.value.trim() };
    if (connApiKey.value) body.api_key = connApiKey.value; // 비어 있으면 생략 → 서버 세션 키 유지
    apiPost('/api/connection', body).then(function (res) {
      state.checking = false;
      connTestBtn.disabled = false;
      var d = res.data || {};
      if (res.ok && d.ok) {
        state.lastCheckOk = true;
        state.lastAvailable = Array.isArray(d.available_models) ? d.available_models : [];
        if (typeof d.base_url === 'string' && d.base_url) connBaseUrl.value = d.base_url;
        state.keyConfigured = !!d.key_configured;
        connApiKey.value = '';
        setKeyHint();
        connStatus.className = 'conn-status ok';
        setText(connStatus, '연결 성공 · ' + state.lastAvailable.length + '개 모델 확인');
        renderAvailable();
        updateGenerateState();
        var availMsg = availabilityBlocked();
        if (availMsg) {
          setConnDetails(true, '· 확인 필요');
          setFormStatus(availMsg, true);
        } else {
          setConnDetails(false, '· 연결됨');
          setFormStatus(auto ? '서버 연결이 확인되었습니다. 바로 입력하세요.' : '연결 확인 성공. 선택한 모델을 사용할 수 있습니다.');
        }
      } else {
        state.lastCheckOk = false;
        state.lastAvailable = null;
        renderAvailable();
        connStatus.className = 'conn-status bad';
        setText(connStatus, '연결 실패');
        setText(connError, (d && d.error) ? String(d.error) : '연결 확인에 실패했습니다 (HTTP ' + res.status + ').');
        show(connError);
        setConnDetails(true, '· 확인 필요');
        updateGenerateState();
      }
    }).catch(function (e) {
      state.checking = false;
      connTestBtn.disabled = false;
      state.lastCheckOk = false;
      state.lastAvailable = null;
      renderAvailable();
      connStatus.className = 'conn-status bad';
      setText(connStatus, '연결 실패');
      setText(connError, e.message || '연결 확인 중 오류가 발생했습니다.');
      show(connError);
      if (!auto) setConnDetails(true, '· 확인 필요');
      updateGenerateState();
      if (/Failed to fetch|NetworkError|Load failed/i.test(e.message || '')) markServerLost();
    });
  }
  function invalidateCheck() {
    if (!state.lastCheckOk && state.lastAvailable === null) return;
    state.lastCheckOk = false;
    state.lastAvailable = null;
    renderAvailable();
    setConnDetails(true, '· 확인 필요');
    updateGenerateState();
  }
  // ---------- 서버 종료 ----------
  function shutdownServer() {
    if (state.activeJobId || state.submitting) {
      setText(shutdownStatus, '생성 중에는 종료할 수 없습니다.');
      return;
    }
    if (state.serverLost || state.shuttingDown) return;
    var ok = window.confirm('로컬 이미지 서버를 종료할까요?\n\n- 이 PC에서 실행 중인 서버만 멈춥니다.\n- 생성된 이미지 파일은 삭제되지 않습니다.\n- 종료 후에는 open-image-studio.cmd를 다시 실행해야 합니다.');
    if (!ok) return;
    state.shuttingDown = true;
    updateGenerateState();
    setText(shutdownStatus, '종료 요청 중…');
    apiPost('/api/shutdown', {}).then(function (res) {
      var d = res.data || {};
      if (res.status === 409) {
        state.shuttingDown = false;
        setText(shutdownStatus, '생성 중인 작업이 있어 종료가 거부되었습니다.');
        updateGenerateState();
        return fetchJobs();
      }
      if (res.ok && d.ok) {
        state.shuttingDown = false;
        state.activeJobId = null;
        state.serverLost = true;
        stopPolling();
        if (elapsedTimer) { clearInterval(elapsedTimer); elapsedTimer = null; }
        updateGenerateState();
        setServerState('bad', '서버 종료됨');
        showBanner('서버를 종료했습니다. 이미지는 삭제되지 않았습니다. 다시 시작하려면 open-image-studio.cmd를 실행하세요.');
        setText(shutdownStatus, '종료됨. open-image-studio.cmd로 다시 시작하세요.');
        setFormStatus('서버가 종료되었습니다. 다시 시작하려면 open-image-studio.cmd를 실행하세요.', true);
        return;
      }
      state.shuttingDown = false;
      setText(shutdownStatus, (d && d.error) ? String(d.error) : '종료 요청이 거부되었습니다 (HTTP ' + res.status + ').');
      updateGenerateState();
    }).catch(function (e) {
      state.shuttingDown = false;
      state.activeJobId = null;
      stopPolling();
      if (elapsedTimer) { clearInterval(elapsedTimer); elapsedTimer = null; }
      updateGenerateState();
      setServerState('bad', '연결 끊김');
      showBanner('서버 연결이 끊겼습니다. 종료 요청이 전달됐는지 알 수 없으니 서버 콘솔을 확인하세요. 이미지는 삭제되지 않습니다.');
      setText(shutdownStatus, '상태 불명 — 서버 콘솔을 확인하세요.');
      setFormStatus('서버 연결이 끊겼습니다. 종료 여부는 서버 콘솔에서 확인하세요.', true);
    });
  }

  // ---------- 생성 제출 ----------
  function submitGenerate(ev) {
    ev.preventDefault();
    hide(formError);
    if (state.submitting || state.activeJobId) {
      showFormError('이미 생성이 진행 중입니다. 중복 결제를 막기 위해 한 번에 한 작업만 허용합니다.');
      return;
    }
    if (!state.configLoaded) {
      showFormError('설정을 불러오는 중입니다. 잠시 후 다시 시도하세요.');
      return;
    }
    if (state.serverLost || state.shuttingDown) {
      showFormError('서버와 연결되지 않았습니다. 서버를 시작한 뒤 새로고침하세요.');
      return;
    }
    if (!state.lastCheckOk) {
      showFormError('연결 확인을 먼저 성공시키세요. [연결 설정]에서 [연결 확인]을 실행하세요.');
      return;
    }
    var prompt = promptInput.value.trim();
    var positive = positiveInput.value.trim();
    var negative = negativeInput.value.trim();
    var imageModel = modelSelect.value;
    var size = sizeSelect.value;
    var quality = qualitySelect.value;
    if (!prompt && !positive) {
      showFormError('메인 프롬프트 또는 Positive 중 하나는 반드시 입력하세요.');
      return;
    }
    if (!state.modelsById[imageModel]) {
      showFormError('알 수 없는 모델입니다: ' + imageModel + '. 목록에서 다시 선택하세요.');
      return;
    }
    if (modelSizes(state.modelsById[imageModel]).indexOf(size) < 0) {
      showFormError('선택한 모델에서 지원하지 않는 해상도입니다: ' + size + '. 목록에서 다시 선택하세요. (직접 입력은 지원하지 않습니다)');
      return;
    }
    if (modelQualities(state.modelsById[imageModel]).indexOf(quality) < 0) {
      showFormError('선택한 모델에서 지원하지 않는 품질입니다: ' + quality + '. 목록에서 다시 선택하세요.');
      return;
    }
    if (grokBlocked()) {
      showFormError('선택한 모델(' + imageModel + ')은 참조 이미지를 지원하지 않습니다. 참조를 모두 제거하거나 참조 가능한 모델로 바꾸세요. 첨부 파일은 자동 삭제하지 않았습니다.');
      return;
    }
    var availMsg = availabilityBlocked();
    if (availMsg) {
      showFormError(availMsg);
      return;
    }
    var body = {
      prompt: prompt,
      positive: positive,
      negative: negative,
      image_model: imageModel,
      size: size,
      quality: quality,
      references: state.refs.map(function (r) { return { name: r.name, data_url: r.data_url }; })
    };
    state.submitting = true;
    updateGenerateState();
    setFormStatus('생성 요청을 보내는 중…');
    apiPost('/api/jobs', body).then(function (res) {
      if (res.status === 409) {
        state.submitting = false;
        setFormStatus('이미 생성 중인 작업이 있습니다. 현재 작업을 먼저 완료하세요.', true);
        showFormError('서버가 409를 반환했습니다: 다른 작업이 아직 진행 중입니다. [목록 새로고침] 후 진행 상황을 확인하세요.');
        return fetchJobs().then(function () {
          updateGenerateState();
        });
      }
      var job = res.data || {};
      if ((res.status === 200 || res.status === 201 || res.status === 202) && job && job.id) {
        state.submitting = false;
        state.activeJobId = job.id;
        state.selectedJobId = job.id;
        state.jobsById[job.id] = job;
        var exists = state.jobs.some(function (j) { return j.id === job.id; });
        if (!exists) state.jobs.unshift(job);
        saveDraft();
        renderJobs();
        renderCurrent();
        startPolling();
        updateGenerateState();
      } else {
        state.submitting = false;
        var msg = (job && (job.error || job.message)) || ('생성 요청이 거부되었습니다 (HTTP ' + res.status + ').');
        showFormError(String(msg));
        setFormStatus('생성 요청이 거부되었습니다.', true);
        updateGenerateState();
      }
    }).catch(function (e) {
      state.submitting = false;
      showFormError(e.message || '생성 요청 중 오류가 발생했습니다.');
      if (/Failed to fetch|NetworkError|Load failed/i.test(e.message || '')) markServerLost();
      updateGenerateState();
    });
  }

  // ---------- 다시 쓰기 ----------
  function reuseSelected() {
    var job = state.selectedJobId ? state.jobsById[state.selectedJobId] : null;
    if (!job) return;
    promptInput.value = job.prompt || '';
    positiveInput.value = job.positive || '';
    negativeInput.value = job.negative || '';
    // 호환 only: 카탈로그에 있을 때만 복사
    var notes = [];
    if (job.image_model && state.modelsById[job.image_model]) {
      modelSelect.value = job.image_model;
    } else if (job.image_model) {
      notes.push('모델(' + job.image_model + ')은 현재 카탈로그에 없어 유지했습니다.');
    }
    renderSizeQuality(job.size && modelSizes(state.modelsById[modelSelect.value]).indexOf(job.size) >= 0 ? job.size : null,
      job.quality && modelQualities(state.modelsById[modelSelect.value]).indexOf(job.quality) >= 0 ? job.quality : null);
    renderModelNote();
    if (job.reference_count) {
      notes.push('참조 이미지는 다시 첨부해야 합니다 (저장된 참조는 불러오지 않음).');
    }
    saveDraft();
    updateGenerateState();
    setFormStatus('설정을 불러왔습니다. ' + (notes.length ? notes.join(' ') : '참조가 필요하면 직접 다시 첨부하세요.'));
    promptInput.focus();
  }

  // ---------- 이벤트 ----------
  function bind() {
    $('editorToggle').addEventListener('click', function () {
      var expanded = $('studioLayout').classList.toggle('editor-wide');
      this.setAttribute('aria-pressed', String(expanded));
      setText(this, expanded ? '결과와 나란히 보기' : '입력칸 넓게 보기');
    });
    bannerRefreshBtn.addEventListener('click', function () { window.location.reload(); });
    connTestBtn.addEventListener('click', testConnection);
    if (shutdownBtn) shutdownBtn.addEventListener('click', shutdownServer);
    genForm.addEventListener('submit', submitGenerate);
    modelSelect.addEventListener('change', function () {
      // 참조는 절대 버리지 않음: 경고/버튼 상태만 갱신
      var keepSize = sizeSelect.value;
      var keepQuality = qualitySelect.value;
      renderSizeQuality(keepSize, keepQuality);
      renderModelNote();
      updateGenerateState();
      scheduleDraftSave();
      var availMsg = availabilityBlocked();
      if (availMsg) setFormStatus(availMsg, true);
      else if (grokBlocked()) setFormStatus('참조가 첨부된 채로 참조 미지원 모델이 선택되어 있습니다. 생성이 막혀 있습니다.', true);
      else setFormStatus('');
    });
    sizeSelect.addEventListener('change', scheduleDraftSave);
    qualitySelect.addEventListener('change', scheduleDraftSave);
    promptInput.addEventListener('input', scheduleDraftSave);
    positiveInput.addEventListener('input', scheduleDraftSave);
    negativeInput.addEventListener('input', scheduleDraftSave);
    connBaseUrl.addEventListener('input', invalidateCheck);
    connApiKey.addEventListener('input', invalidateCheck);
    dropZone.addEventListener('click', function () { refInput.click(); });
    dropZone.addEventListener('keydown', function (ev) {
      if (ev.key === 'Enter' || ev.key === ' ') {
        ev.preventDefault();
        refInput.click();
      }
    });
    refInput.addEventListener('change', function () {
      addFiles(refInput.files);
      refInput.value = '';
    });
    ['dragenter', 'dragover'].forEach(function (t) {
      dropZone.addEventListener(t, function (ev) {
        ev.preventDefault();
        dropZone.classList.add('dragging');
      });
    });
    ['dragleave', 'drop'].forEach(function (t) {
      dropZone.addEventListener(t, function (ev) {
        ev.preventDefault();
        dropZone.classList.remove('dragging');
      });
    });
    dropZone.addEventListener('drop', function (ev) {
      var files = ev.dataTransfer ? ev.dataTransfer.files : [];
      addFiles(files);
    });
    reuseBtn.addEventListener('click', reuseSelected);
    refreshJobsBtn.addEventListener('click', function () {
      setFormStatus('작업 목록을 새로고침하는 중…');
      fetchJobs().then(function () {
        setFormStatus('작업 목록을 갱신했습니다.');
      }).catch(function (e) {
        setFormStatus(e.message || '목록 새로고침 실패.', true);
      });
    });
    window.addEventListener('beforeunload', saveDraft);
  }

  // ---------- 초기화 ----------
  function init() {
    renderModelOptions(null);
    renderSizeQuality(null, null);
    renderModelNote();
    renderLimits();
    renderAvailable();
    renderRefs();
    bind();
    updateGenerateState();
    setServerState('', '서버 확인 중…');
    // 저장된 설정 우선: health → config → jobs
    fetchHealth().then(function () {
      return fetchConfig().catch(function (e) {
        markServerLost('설정을 불러오지 못했습니다: ' + (e.message || e) + '. 서버를 시작한 뒤 새로고침하세요.');
      });
    }).then(function () {
      if (!state.serverLost) {
        setServerState('ok', '서버 연결됨');
        hideBanner();
        state.serverLost = false;
      }
      return fetchJobs().catch(function () {
        if (!state.serverLost) markServerLost();
      });
    }).then(function () {
      updateGenerateState();
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
