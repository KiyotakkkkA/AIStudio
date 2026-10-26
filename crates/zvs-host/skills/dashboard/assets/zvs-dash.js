(function () {
  "use strict";

  const SVG_NS = "http://www.w3.org/2000/svg";
  const MAX_SERIES = 8;
  const SCATTER_SERIES = 3;
  const DONUT_SEGMENTS = 6;
  const BAR_MAX = 24;
  const GAP = 2;
  const SEQUENTIAL = [
    "#fbf1e8",
    "#f8e3d2",
    "#f4d3ba",
    "#efc2a1",
    "#e9b088",
    "#e29d70",
    "#d98958",
    "#cd7341",
    "#c2571c",
    "#a44818",
    "#863b14",
    "#692e10",
    "#4d220c",
  ];
  const ORDINAL = SEQUENTIAL.slice(3, 11);
  const DARK_LABEL_FROM = 7;
  const CHART_TYPES = ["line", "area", "bar", "donut", "scatter", "heatmap", "funnel"];

  const COPY = {
    ru: {
      locale: "ru-RU",
      all: "Все",
      table: "Таблица",
      chart: "График",
      other: "Прочее",
      search: "Поиск",
      shown: (shown, total) => `Показано ${shown} из ${total}`,
      showAll: "Показать все",
      empty: "Нет данных для выбранных фильтров",
      from: "с",
      to: "по",
      theme: "Тема",
      themes: { auto: "Авто", light: "Светлая", dark: "Тёмная" },
      view: "Вид",
      pickDate: "Выбрать дату",
      prevMonth: "Предыдущий месяц",
      nextMonth: "Следующий месяц",
      clear: "Сбросить",
      nothing: "Ничего не найдено",
      generated: "Сформировано",
      sources: "Источники",
      reset: "Сбросить фильтры",
      total: "Всего",
      ofFirst: "от первого этапа",
      fromPrevious: "от предыдущего",
      target: "Цель",
      failed: "Не удалось построить блок",
      noValue: "—",
    },
    en: {
      locale: "en-US",
      all: "All",
      table: "Table",
      chart: "Chart",
      other: "Other",
      search: "Search",
      shown: (shown, total) => `Showing ${shown} of ${total}`,
      showAll: "Show all",
      empty: "No data for the selected filters",
      from: "from",
      to: "to",
      theme: "Theme",
      themes: { auto: "Auto", light: "Light", dark: "Dark" },
      view: "View",
      pickDate: "Pick a date",
      prevMonth: "Previous month",
      nextMonth: "Next month",
      clear: "Clear",
      nothing: "Nothing found",
      generated: "Generated",
      sources: "Sources",
      reset: "Reset filters",
      total: "Total",
      ofFirst: "of the first stage",
      fromPrevious: "from the previous",
      target: "Target",
      failed: "Could not render this block",
      noValue: "—",
    },
  };

  const state = {
    spec: null,
    data: {},
    filters: {},
    copy: COPY.ru,
    cards: [],
    formats: new Map(),
  };

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined && text !== null) node.textContent = String(text);
    return node;
  }

  function svg(tag, attrs, parent) {
    const node = document.createElementNS(SVG_NS, tag);
    for (const key of Object.keys(attrs || {})) {
      const value = attrs[key];
      if (value !== undefined && value !== null) node.setAttribute(key, String(value));
    }
    if (parent) parent.appendChild(node);
    return node;
  }

  const ICON_PATHS = {
    chevron: "M6 9l6 6 6-6",
    check: "M5 12.5l4.5 4.5L19 7.5",
    search: "M11 4.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM20 20l-4.2-4.2",
    calendar:
      "M8 3v3M16 3v3M4 9.5h16M6 5h12a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2z",
    left: "M15 18l-6-6 6-6",
    right: "M9 18l6-6-6-6",
  };

  function icon(name, size) {
    const node = svg("svg", {
      class: "zd-icon",
      width: size || 14,
      height: size || 14,
      viewBox: "0 0 24 24",
      "aria-hidden": "true",
    });
    svg("path", { d: ICON_PATHS[name] }, node);
    return node;
  }

  function button(content, tone, size) {
    const node = el(
      "button",
      `zd-btn zd-btn-${tone || "secondary"}${size === "sm" ? " zd-btn-sm" : ""}`,
    );
    node.type = "button";
    if (content !== undefined && content !== null) node.append(content);
    return node;
  }

  function segmented(options, value, label, onChange, small) {
    const group = el("div", `zd-seg${small ? " zd-seg-sm" : ""}`);
    group.setAttribute("role", "radiogroup");
    group.setAttribute("aria-label", label);
    let current = value;
    const items = options.map((option) => {
      const item = el("button", "zd-seg-item", option.label);
      item.type = "button";
      item.setAttribute("role", "radio");
      item.addEventListener("click", () => {
        if (option.value === current) return;
        set(option.value);
        onChange(option.value);
      });
      group.appendChild(item);
      return item;
    });
    function set(next) {
      current = next;
      items.forEach((item, index) => {
        const checked = options[index].value === next;
        item.setAttribute("aria-checked", String(checked));
        item.tabIndex = checked ? 0 : -1;
      });
    }
    group.addEventListener("keydown", (event) => {
      const index = Math.max(
        0,
        options.findIndex((option) => option.value === current),
      );
      const steps = { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 };
      let next = null;
      if (event.key in steps) next = (index + steps[event.key] + options.length) % options.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = options.length - 1;
      if (next === null) return;
      event.preventDefault();
      items[next].focus();
      items[next].click();
    });
    set(value);
    return { node: group, set };
  }

  function searchField(placeholder, value, onInput, small) {
    const field = el("div", "zd-field");
    const input = el("input", `zd-input${small ? " zd-input-sm" : ""}`);
    input.type = "search";
    input.placeholder = placeholder;
    input.value = value || "";
    input.setAttribute("aria-label", placeholder);
    input.addEventListener("input", () => onInput(input.value));
    field.append(icon("search", small ? 13 : 14), input);
    return { node: field, input };
  }

  const popover = { owner: null, close: null };

  function closePopover() {
    const close = popover.close;
    popover.owner = null;
    popover.close = null;
    if (close) close();
  }

  function openPopover(owner, trigger, menu) {
    closePopover();
    menu.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    delete menu.dataset.placement;
    delete menu.dataset.align;
    const rect = trigger.getBoundingClientRect();
    const height = menu.offsetHeight;
    const width = menu.offsetWidth;
    if (rect.bottom + height + 12 > window.innerHeight && rect.top - height - 12 > 0)
      menu.dataset.placement = "top";
    if (rect.left + width + 8 > window.innerWidth) menu.dataset.align = "end";
    popover.owner = owner;
    popover.close = () => {
      menu.hidden = true;
      trigger.setAttribute("aria-expanded", "false");
    };
  }

  document.addEventListener("pointerdown", (event) => {
    if (popover.owner && !popover.owner.contains(event.target)) closePopover();
  });

  let uid = 0;

  function select(config) {
    uid += 1;
    const id = `zd-select-${uid}`;
    const root = el("div", "zd-select");
    const trigger = el("button", "zd-select-trigger");
    trigger.type = "button";
    trigger.setAttribute("aria-haspopup", "listbox");
    trigger.setAttribute("aria-expanded", "false");
    trigger.setAttribute("aria-controls", id);
    trigger.setAttribute("aria-label", config.label);
    const shown = el("span", "zd-select-value");
    trigger.append(shown, icon("chevron"));
    const menu = el("div", "zd-menu");
    menu.hidden = true;
    const list = el("div", "zd-options");
    list.id = id;
    list.setAttribute("role", "listbox");
    list.setAttribute("aria-label", config.label);
    let value = config.value;
    let needle = "";
    let active = 0;
    let visible = [];
    let query = null;
    if (config.options.length > 10) {
      const box = el("div", "zd-menu-search");
      const field = searchField(
        state.copy.search,
        "",
        (text) => {
          needle = text;
          active = 0;
          draw();
        },
        true,
      );
      box.appendChild(field.node);
      menu.appendChild(box);
      query = field.input;
    }
    menu.appendChild(list);
    root.append(trigger, menu);

    function labelOf(current) {
      const found = config.options.find((option) => option.value === current);
      return found ? found.label : "";
    }

    function sync() {
      const text = labelOf(value);
      shown.textContent = text || config.placeholder || "";
      shown.dataset.empty = String(!text);
    }

    function highlight(index) {
      active = index;
      [...list.children].forEach((item, position) => {
        item.dataset.active = String(position === index);
      });
      const node = list.children[index];
      if (node && node.scrollIntoView) node.scrollIntoView({ block: "nearest" });
      (query || trigger).setAttribute("aria-activedescendant", `${id}-${index}`);
    }

    function draw() {
      const text = needle.trim().toLowerCase();
      visible = config.options.filter(
        (option) => !text || option.label.toLowerCase().includes(text),
      );
      list.replaceChildren();
      if (visible.length === 0) {
        list.appendChild(el("p", "zd-menu-empty", state.copy.nothing));
        return;
      }
      active = Math.max(0, Math.min(active, visible.length - 1));
      visible.forEach((option, index) => {
        const item = el("div", "zd-option");
        item.id = `${id}-${index}`;
        item.setAttribute("role", "option");
        item.setAttribute("aria-selected", String(option.value === value));
        item.dataset.active = String(index === active);
        item.append(el("span", "", option.label), icon("check"));
        item.addEventListener("pointermove", () => {
          if (active !== index) highlight(index);
        });
        item.addEventListener("click", () => choose(option));
        list.appendChild(item);
      });
    }

    function choose(option) {
      value = option.value;
      sync();
      closePopover();
      trigger.focus();
      config.onChange(value);
    }

    function open() {
      needle = "";
      if (query) query.value = "";
      active = Math.max(
        0,
        config.options.findIndex((option) => option.value === value),
      );
      draw();
      openPopover(root, trigger, menu);
      if (query) query.focus();
      highlight(active);
    }

    function keys(event) {
      if (menu.hidden) {
        if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
          event.preventDefault();
          open();
        }
        return;
      }
      if (event.key === "ArrowDown") {
        event.preventDefault();
        highlight(Math.min(visible.length - 1, active + 1));
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        highlight(Math.max(0, active - 1));
      } else if (event.key === "Enter") {
        event.preventDefault();
        if (visible[active]) choose(visible[active]);
      } else if (event.key === "Escape") {
        event.preventDefault();
        closePopover();
        trigger.focus();
      } else if (event.key === "Tab") {
        closePopover();
      } else if (!query && event.key.length === 1) {
        const key = event.key.toLowerCase();
        const count = visible.length;
        for (let step = 1; step <= count; step += 1) {
          const index = (active + step) % count;
          if (visible[index].label.toLowerCase().startsWith(key)) {
            highlight(index);
            break;
          }
        }
      }
    }

    trigger.addEventListener("click", () => {
      if (menu.hidden) open();
      else closePopover();
    });
    trigger.addEventListener("keydown", keys);
    if (query) query.addEventListener("keydown", keys);
    sync();
    return {
      node: root,
      set(next) {
        value = next;
        sync();
      },
    };
  }

  function isoOf(date) {
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${date.getFullYear()}-${month}-${day}`;
  }

  function parseDay(text) {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(text || ""));
    return match ? new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3])) : null;
  }

  function datePicker(config) {
    const root = el("div", "zd-select zd-date");
    const trigger = el("button", "zd-select-trigger");
    trigger.type = "button";
    trigger.setAttribute("aria-haspopup", "dialog");
    trigger.setAttribute("aria-expanded", "false");
    trigger.setAttribute("aria-label", config.label);
    const shown = el("span", "zd-select-value");
    trigger.append(shown, icon("calendar"));
    const menu = el("div", "zd-menu zd-calendar");
    menu.hidden = true;
    menu.setAttribute("role", "dialog");
    menu.setAttribute("aria-label", config.label);
    root.append(trigger, menu);
    const locale = state.copy.locale;
    const first = locale.startsWith("en") ? 0 : 1;
    const dayFormat = new Intl.DateTimeFormat(locale, {
      day: "numeric",
      month: "short",
      year: "numeric",
    });
    const monthFormat = new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" });
    const weekdayFormat = new Intl.DateTimeFormat(locale, { weekday: "short" });
    let value = config.value || "";
    let view = new Date();
    let focus = isoOf(view);

    function sync() {
      const day = parseDay(value);
      shown.textContent = day ? dayFormat.format(day) : config.placeholder || state.copy.pickDate;
      shown.dataset.empty = String(!day);
    }

    function allowed(iso) {
      return (!config.min || iso >= config.min) && (!config.max || iso <= config.max);
    }

    function shift(months) {
      view = new Date(view.getFullYear(), view.getMonth() + months, 1);
      const day = parseDay(focus);
      const last = new Date(view.getFullYear(), view.getMonth() + 1, 0).getDate();
      focus = isoOf(new Date(view.getFullYear(), view.getMonth(), Math.min(day.getDate(), last)));
      draw();
    }

    function draw() {
      menu.replaceChildren();
      const head = el("div", "zd-calendar-head");
      const previous = button(icon("left"), "ghost", "sm");
      previous.classList.add("zd-btn-icon");
      previous.setAttribute("aria-label", state.copy.prevMonth);
      previous.addEventListener("click", () => shift(-1));
      const next = button(icon("right"), "ghost", "sm");
      next.classList.add("zd-btn-icon");
      next.setAttribute("aria-label", state.copy.nextMonth);
      next.addEventListener("click", () => shift(1));
      const month = monthFormat.format(view).replace(/\s*г\.$/, "");
      head.append(
        previous,
        el("span", "zd-calendar-month", month.charAt(0).toUpperCase() + month.slice(1)),
        next,
      );
      const grid = el("div", "zd-calendar-grid");
      grid.setAttribute("role", "grid");
      for (let index = 0; index < 7; index += 1) {
        grid.appendChild(
          el(
            "span",
            "zd-weekday",
            weekdayFormat.format(new Date(2024, 0, 7 + ((first + index) % 7))),
          ),
        );
      }
      const start = new Date(view.getFullYear(), view.getMonth(), 1);
      const offset = (start.getDay() - first + 7) % 7;
      const today = isoOf(new Date());
      for (let index = 0; index < 42; index += 1) {
        const date = new Date(view.getFullYear(), view.getMonth(), 1 - offset + index);
        const iso = isoOf(date);
        const day = el("button", "zd-day", date.getDate());
        day.type = "button";
        day.dataset.iso = iso;
        day.dataset.outside = String(date.getMonth() !== view.getMonth());
        if (iso === today) day.dataset.today = "true";
        day.setAttribute("aria-selected", String(iso === value));
        day.setAttribute("aria-label", dayFormat.format(date));
        day.disabled = !allowed(iso);
        day.tabIndex = iso === focus ? 0 : -1;
        day.addEventListener("click", () => pick(iso));
        grid.appendChild(day);
      }
      grid.addEventListener("keydown", (event) => {
        const steps = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 };
        if (!(event.key in steps)) return;
        event.preventDefault();
        const current = parseDay(focus);
        const target = new Date(
          current.getFullYear(),
          current.getMonth(),
          current.getDate() + steps[event.key],
        );
        focus = isoOf(target);
        if (target.getMonth() !== view.getMonth() || target.getFullYear() !== view.getFullYear())
          view = new Date(target.getFullYear(), target.getMonth(), 1);
        draw();
        const node = menu.querySelector(`[data-iso="${focus}"]`);
        if (node) node.focus();
      });
      const foot = el("div", "zd-calendar-foot");
      const clear = button(state.copy.clear, "ghost", "sm");
      clear.addEventListener("click", () => pick(""));
      foot.appendChild(clear);
      menu.append(head, grid, foot);
    }

    function pick(iso) {
      value = iso;
      sync();
      closePopover();
      trigger.focus();
      config.onChange(value);
    }

    function open() {
      const today = isoOf(new Date());
      const base =
        parseDay(value) ||
        parseDay(config.max && today > config.max ? config.max : "") ||
        parseDay(config.min && today < config.min ? config.min : "") ||
        new Date();
      view = new Date(base.getFullYear(), base.getMonth(), 1);
      focus = isoOf(base);
      draw();
      openPopover(root, trigger, menu);
      const node = menu.querySelector(`[data-iso="${focus}"]`);
      if (node && !node.disabled) node.focus();
    }

    trigger.addEventListener("click", () => {
      if (menu.hidden) open();
      else closePopover();
    });
    trigger.addEventListener("keydown", (event) => {
      if (menu.hidden && event.key === "ArrowDown") {
        event.preventDefault();
        open();
      }
    });
    menu.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closePopover();
        trigger.focus();
      }
    });
    sync();
    return {
      node: root,
      set(next) {
        value = next || "";
        sync();
      },
    };
  }

  function color(slot) {
    return `var(--s${(slot % MAX_SERIES) + 1})`;
  }

  let measureContext = null;

  function textWidth(text, size) {
    const value = String(text);
    if (measureContext === null) {
      try {
        measureContext = document.createElement("canvas").getContext("2d") || false;
      } catch {
        measureContext = false;
      }
    }
    if (measureContext) {
      measureContext.font = `${size}px system-ui, -apple-system, "Segoe UI", sans-serif`;
      return measureContext.measureText(value).width;
    }
    return value.length * size * 0.58;
  }

  function truncate(text, size, width) {
    const value = String(text);
    if (textWidth(value, size) <= width) return value;
    let end = value.length;
    while (end > 1 && textWidth(`${value.slice(0, end)}…`, size) > width) end -= 1;
    return `${value.slice(0, end)}…`;
  }

  function isBlank(value) {
    return value === null || value === undefined || value === "";
  }

  function timeOf(value) {
    if (isBlank(value)) return NaN;
    let text = String(value);
    if (/^\d{4}$/.test(text)) text += "-01-01";
    else if (/^\d{4}-\d{2}$/.test(text)) text += "-01";
    else if (/T\d{2}:\d{2}/.test(text) && !/(Z|[+-]\d{2}:?\d{2})$/.test(text)) text += "Z";
    else if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}/.test(text)) text = `${text.replace(" ", "T")}Z`;
    return Date.parse(text);
  }

  function dayOf(value) {
    const text = String(value ?? "").slice(0, 10);
    if (/^\d{4}$/.test(text)) return `${text}-01-01`;
    if (/^\d{4}-\d{2}$/.test(text)) return `${text}-01`;
    return text;
  }

  function compareBy(type) {
    if (type === "date") return (a, b) => timeOf(a) - timeOf(b);
    if (type === "number") return (a, b) => Number(a) - Number(b);
    return (a, b) => String(a).localeCompare(String(b), state.copy.locale);
  }

  function formatter(kind) {
    const key = kind || "auto";
    if (state.formats.has(key)) return state.formats.get(key);
    const locale = state.copy.locale;
    const number = (options) => {
      const format = new Intl.NumberFormat(locale, options);
      return (value) => format.format(value);
    };
    let format;
    if (key === "integer") format = number({ maximumFractionDigits: 0 });
    else if (key === "decimal") format = number({ maximumFractionDigits: 2 });
    else if (key === "percent") format = number({ style: "percent", maximumFractionDigits: 1 });
    else if (key === "percentValue") {
      const percent = number({ style: "percent", maximumFractionDigits: 1 });
      format = (value) => percent(value / 100);
    } else if (key === "compact")
      format = number({ notation: "compact", maximumFractionDigits: 1 });
    else if (key.startsWith("currency:")) {
      const currency = key.slice(9) || "RUB";
      const whole = number({ style: "currency", currency, maximumFractionDigits: 0 });
      const cents = number({ style: "currency", currency, maximumFractionDigits: 2 });
      format = (value) => (Math.abs(value) >= 100 ? whole(value) : cents(value));
    } else if (["date", "month", "year", "datetime"].includes(key)) {
      const options = {
        date: { day: "2-digit", month: "2-digit", year: "numeric" },
        month: { month: "short", year: "numeric" },
        year: { year: "numeric" },
        datetime: {
          day: "2-digit",
          month: "2-digit",
          year: "numeric",
          hour: "2-digit",
          minute: "2-digit",
        },
      }[key];
      const dates = new Intl.DateTimeFormat(locale, { ...options, timeZone: "UTC" });
      format = (value) => {
        const time = timeOf(value);
        return Number.isFinite(time) ? dates.format(new Date(time)) : String(value);
      };
    } else if (key === "text") format = (value) => String(value);
    else {
      const whole = number({ maximumFractionDigits: 0 });
      const fraction = number({ maximumFractionDigits: 2 });
      format = (value) =>
        typeof value === "number"
          ? Number.isInteger(value)
            ? whole(value)
            : fraction(value)
          : String(value);
    }
    const safe = (value) => {
      if (isBlank(value) || (typeof value === "number" && !Number.isFinite(value)))
        return state.copy.noValue;
      return format(value);
    };
    state.formats.set(key, safe);
    return safe;
  }

  function tickFormatter(kind) {
    const key = kind || "auto";
    if (key === "percent" || key === "percentValue" || key === "integer") return formatter(key);
    if (key.startsWith("currency:")) {
      const format = new Intl.NumberFormat(state.copy.locale, {
        style: "currency",
        currency: key.slice(9) || "RUB",
        notation: "compact",
        maximumFractionDigits: 1,
      });
      return (value) => format.format(value);
    }
    const compact = formatter("compact");
    const plain = formatter("decimal");
    return (value) => (Math.abs(value) >= 1000 ? compact(value) : plain(value));
  }

  function typeOf(dataset, field) {
    const data = state.data[dataset];
    return (data && data.types[field]) || "string";
  }

  function valueFormat(dataset, field, explicit) {
    if (explicit) return explicit;
    const type = typeOf(dataset, field);
    if (type !== "date") return type === "number" ? "auto" : "text";
    const rows = state.data[dataset].rows;
    const sample = rows.slice(0, 50).map((row) => String(row[field] ?? ""));
    if (sample.every((value) => /^\d{4}$/.test(value) || value === "")) return "year";
    if (sample.every((value) => /^\d{4}-\d{2}(-01)?$/.test(value) || value === "")) return "month";
    if (sample.some((value) => /\d{2}:\d{2}/.test(value))) return "datetime";
    return "date";
  }

  function load() {
    const node = document.getElementById("zvs-dashboard-data");
    const spec = JSON.parse(node.textContent);
    state.spec = spec;
    state.copy = COPY[spec.lang] || COPY.ru;
    for (const [name, dataset] of Object.entries(spec.datasets || {})) {
      const columns = dataset.columns || [];
      const types = {};
      for (const column of columns) types[column.name] = column.type || "string";
      const rows = (dataset.rows || []).map((values) => {
        const row = {};
        columns.forEach((column, index) => {
          row[column.name] = values[index];
        });
        return row;
      });
      state.data[name] = { name, columns, types, rows };
    }
  }

  function filterActive(filter) {
    const value = state.filters[filter.id];
    if (filter.type === "dateRange") return Boolean(value && (value.from || value.to));
    return !isBlank(value);
  }

  function filterPasses(filter, value) {
    const selected = state.filters[filter.id];
    if (filter.type === "dateRange") {
      if (isBlank(value)) return false;
      const day = dayOf(value);
      return (!selected.from || day >= selected.from) && (!selected.to || day <= selected.to);
    }
    return String(value) === selected;
  }

  function filterApplies(filter, dataset) {
    const data = state.data[dataset];
    if (!data || !(filter.field in data.types)) return false;
    return !filter.datasets || filter.datasets.includes(dataset);
  }

  function rowsOf(dataset, filtered) {
    const data = state.data[dataset];
    if (!data) throw new Error(`dataset "${dataset}" is missing`);
    if (filtered === false) return data.rows;
    const active = (state.spec.filters || []).filter(
      (filter) => filterApplies(filter, dataset) && filterActive(filter),
    );
    if (active.length === 0) return data.rows;
    return data.rows.filter((row) =>
      active.every((filter) => filterPasses(filter, row[filter.field])),
    );
  }

  function median(values) {
    const sorted = [...values].sort((a, b) => a - b);
    const middle = Math.floor(sorted.length / 2);
    return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
  }

  function aggregate(rows, field, agg) {
    const kind = agg || (field ? "sum" : "count");
    if (kind === "count" && !field) return rows.length;
    const present = rows.map((row) => row[field]).filter((value) => !isBlank(value));
    if (kind === "count") return present.length;
    if (kind === "distinct") return new Set(present.map(String)).size;
    if (kind === "first") return present.length ? present[0] : null;
    if (kind === "last") return present.length ? present[present.length - 1] : null;
    const numbers = present.map(Number).filter(Number.isFinite);
    if (numbers.length === 0) return null;
    if (kind === "sum") return numbers.reduce((total, value) => total + value, 0);
    if (kind === "avg") return numbers.reduce((total, value) => total + value, 0) / numbers.length;
    if (kind === "min") return Math.min(...numbers);
    if (kind === "max") return Math.max(...numbers);
    if (kind === "median") return median(numbers);
    throw new Error(`unknown aggregation "${kind}"`);
  }

  function measure(rows, spec, fallbackAgg, dataset) {
    if (typeof spec === "number") return spec;
    if (spec === null || spec === undefined) return null;
    if (typeof spec === "string") return aggregate(rows, spec, fallbackAgg || "sum");
    if (Array.isArray(spec.ratio)) {
      const top = Number(measure(rows, spec.ratio[0], fallbackAgg, dataset));
      const bottom = Number(measure(rows, spec.ratio[1], fallbackAgg, dataset));
      return Number.isFinite(top) && Number.isFinite(bottom) && bottom !== 0 ? top / bottom : null;
    }
    const source = spec.dataset || dataset;
    let subset = spec.dataset ? rowsOf(spec.dataset) : rows;
    if (spec.where) {
      subset = subset.filter((row) =>
        Object.entries(spec.where).every(([key, value]) =>
          Array.isArray(value)
            ? value.map(String).includes(String(row[key]))
            : String(row[key]) === String(value),
        ),
      );
    }
    if (spec.period && spec.by) {
      const compare = compareBy(typeOf(source, spec.by));
      const keys = [
        ...new Set(
          subset
            .map((row) => row[spec.by])
            .filter((value) => !isBlank(value))
            .map(String),
        ),
      ];
      keys.sort(compare);
      const pick = keys[keys.length - (spec.period === "previous" ? 2 : 1)];
      subset = pick === undefined ? [] : subset.filter((row) => String(row[spec.by]) === pick);
    }
    if (spec.orderBy) {
      const compare = compareBy(typeOf(source, spec.orderBy));
      subset = [...subset].sort((a, b) => compare(a[spec.orderBy], b[spec.orderBy]));
    }
    return aggregate(subset, spec.field, spec.agg || fallbackAgg || (spec.field ? "sum" : "count"));
  }

  function measuresOf(component) {
    const list =
      component.y === undefined ? [] : Array.isArray(component.y) ? component.y : [component.y];
    return list.map((entry, index) =>
      typeof entry === "string"
        ? { field: entry, label: (component.labels && component.labels[index]) || entry }
        : { field: entry.field, label: entry.label || entry.field, agg: entry.agg },
    );
  }

  function keyOf(value) {
    return isBlank(value) ? "—" : String(value);
  }

  function seriesNames(component, rows, cap) {
    const measures = measuresOf(component);
    const field = measures[0] ? measures[0].field : null;
    const totals = new Map();
    for (const row of rows) {
      const key = keyOf(row[component.series]);
      const value = field ? Math.abs(Number(row[field])) : 1;
      totals.set(key, (totals.get(key) || 0) + (Number.isFinite(value) ? value : 0));
    }
    let names = [...totals.entries()].sort((a, b) => b[1] - a[1]).map((entry) => entry[0]);
    if (Array.isArray(component.seriesOrder)) {
      const fixed = component.seriesOrder.map(String).filter((name) => totals.has(name));
      names = fixed.concat(names.filter((name) => !fixed.includes(name)));
    }
    if (names.length > cap) names = names.slice(0, cap - 1).concat([state.copy.other]);
    return names;
  }

  function buildFrame(component, rows, all, cap) {
    const limit = cap || MAX_SERIES;
    const measures = measuresOf(component);
    const defaultAgg = component.agg || (measures.length ? "sum" : "count");
    const groups = new Map();
    const place = (row, name) => {
      const key = keyOf(row[component.x]);
      let group = groups.get(key);
      if (!group) {
        group = { key, x: row[component.x], buckets: new Map(), values: {} };
        groups.set(key, group);
      }
      if (!group.buckets.has(name)) group.buckets.set(name, []);
      group.buckets.get(name).push(row);
    };
    let series;
    if (component.series) {
      const names = seriesNames(component, all, limit);
      const known = new Set(names);
      const measure = measures[0] || { field: null };
      series = names.map((name, slot) => ({
        name,
        label: name,
        slot,
        field: measure.field,
        agg: measure.agg || defaultAgg,
      }));
      for (const row of rows) {
        let name = keyOf(row[component.series]);
        if (!known.has(name)) name = state.copy.other;
        if (known.has(name)) place(row, name);
      }
    } else {
      const list = measures.length ? measures : [{ field: null, label: component.title || "" }];
      series = list.slice(0, limit).map((measure, slot) => ({
        name: measure.field || "count",
        label: measure.label,
        slot,
        field: measure.field,
        agg: measure.agg || defaultAgg,
      }));
      for (const row of rows) for (const entry of series) place(row, entry.name);
    }
    const xs = [...groups.values()];
    for (const group of xs) computeGroup(group, series);
    return { xs, series, xType: typeOf(component.dataset, component.x) };
  }

  function computeGroup(group, series) {
    group.values = {};
    for (const entry of series) {
      const bucket = group.buckets.get(entry.name);
      group.values[entry.name] = bucket ? aggregate(bucket, entry.field, entry.agg) : null;
    }
  }

  function groupTotal(group, series) {
    return series.reduce((total, entry) => total + (Number(group.values[entry.name]) || 0), 0);
  }

  function sortFrame(frame, mode) {
    if (mode === "none") return;
    if (mode === "x") {
      const compare = compareBy(frame.xType);
      frame.xs.sort((a, b) => compare(a.x, b.x));
      return;
    }
    const sign = mode === "asc" ? 1 : -1;
    frame.xs.sort((a, b) => sign * (groupTotal(a, frame.series) - groupTotal(b, frame.series)));
  }

  function foldTail(frame, limit) {
    if (!limit || frame.xs.length <= limit) return;
    const kept = frame.xs.slice(0, limit - 1);
    const other = {
      key: state.copy.other,
      x: state.copy.other,
      buckets: new Map(),
      values: {},
      folded: true,
    };
    for (const group of frame.xs.slice(limit - 1)) {
      for (const [name, rows] of group.buckets) {
        if (!other.buckets.has(name)) other.buckets.set(name, []);
        other.buckets.get(name).push(...rows);
      }
    }
    computeGroup(other, frame.series);
    frame.xs = kept.concat([other]);
  }

  function wholeNumbers(component) {
    return (
      component.format === "integer" || component.agg === "count" || component.agg === "distinct"
    );
  }

  function niceScale(min, max, count, whole) {
    let low = min;
    let high = max;
    if (low === high) {
      if (low === 0) high = 1;
      else if (low > 0) low = 0;
      else high = 0;
    }
    const span = high - low;
    const raw = span / (count || 5);
    const base = Math.pow(10, Math.floor(Math.log10(raw)));
    const ratio = raw / base;
    const nice = base * (ratio >= 7.5 ? 10 : ratio >= 3.5 ? 5 : ratio >= 1.5 ? 2 : 1);
    const step = whole ? Math.max(1, nice) : nice;
    const lo = Math.floor(low / step + 1e-9) * step;
    const hi = Math.ceil(high / step - 1e-9) * step;
    const ticks = [];
    for (let value = lo; value <= hi + step / 2; value += step)
      ticks.push(Number(value.toFixed(10)));
    return { lo, hi, ticks, map: (value, a, b) => b - ((value - lo) / (hi - lo)) * (b - a) };
  }

  function linear(lo, hi, a, b) {
    return (value) => a + ((value - lo) / (hi - lo || 1)) * (b - a);
  }

  const tooltip = { node: null };

  function tip() {
    if (tooltip.node === null) {
      tooltip.node = el("div", "zd-tooltip");
      tooltip.node.setAttribute("role", "status");
      tooltip.node.hidden = true;
      document.body.appendChild(tooltip.node);
    }
    return tooltip.node;
  }

  function showTip(point, title, rows) {
    const node = tip();
    node.replaceChildren();
    if (title) node.appendChild(el("div", "zd-tip-title", title));
    for (const row of rows) {
      const line = el("div", "zd-tip-row");
      if (row.color) {
        const key = el("span", `zd-key zd-key-${row.mark || "line"}`);
        key.style.background = row.color;
        line.appendChild(key);
      }
      line.appendChild(el("strong", "zd-tip-value", row.value));
      if (row.label) line.appendChild(el("span", "zd-tip-label", row.label));
      node.appendChild(line);
    }
    node.hidden = false;
    const width = node.offsetWidth || 160;
    const height = node.offsetHeight || 60;
    let left = point.x + 14;
    let top = point.y + 14;
    if (left + width > window.innerWidth - 8) left = point.x - width - 14;
    if (top + height > window.innerHeight - 8) top = point.y - height - 14;
    node.style.left = `${Math.max(8, left)}px`;
    node.style.top = `${Math.max(8, top)}px`;
  }

  function hideTip() {
    if (tooltip.node) tooltip.node.hidden = true;
  }

  function pointOf(event, fallback) {
    if (event && typeof event.clientX === "number" && (event.clientX || event.clientY)) {
      return { x: event.clientX, y: event.clientY };
    }
    const rect = fallback.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top };
  }

  function bindHover(target, onShow, onHide) {
    target.setAttribute("tabindex", "0");
    target.addEventListener("pointermove", (event) => onShow(event));
    target.addEventListener("pointerleave", () => {
      hideTip();
      if (onHide) onHide();
    });
    target.addEventListener("focus", (event) => onShow(event));
    target.addEventListener("blur", () => {
      hideTip();
      if (onHide) onHide();
    });
  }

  function widthOf(entry) {
    const width = entry.body.clientWidth;
    return width > 0 ? width : 640;
  }

  function plotArea(entry, options) {
    const width = widthOf(entry);
    const tickWidth = Math.max(0, ...options.yLabels.map((label) => textWidth(label, 11)));
    const area = {
      width,
      height: options.height,
      left: Math.ceil(tickWidth) + 12,
      right: width - (options.right || 12),
      top: options.top || 12,
      bottom: options.height - (options.bottom || 28),
    };
    area.svg = svg("svg", {
      class: "zd-svg",
      width,
      height: options.height,
      viewBox: `0 0 ${width} ${options.height}`,
      role: "img",
      "aria-label": entry.c.title || entry.c.type,
    });
    entry.body.appendChild(area.svg);
    return area;
  }

  function drawYAxis(area, scale, format) {
    const group = svg("g", { class: "zd-axis" }, area.svg);
    for (const value of scale.ticks) {
      const y = scale.map(value, area.top, area.bottom);
      svg(
        "line",
        {
          x1: area.left,
          x2: area.right,
          y1: y,
          y2: y,
          class: value === 0 ? "zd-base" : "zd-gridline",
        },
        group,
      );
      const label = svg(
        "text",
        { x: area.left - 8, y: y + 4, "text-anchor": "end", class: "zd-tick" },
        group,
      );
      label.textContent = format(value);
    }
  }

  function drawXLabels(area, labels, positions) {
    const group = svg("g", { class: "zd-axis" }, area.svg);
    const widest = Math.max(1, ...labels.map((label) => textWidth(label, 11)));
    const room = area.right - area.left;
    const fits = Math.max(1, Math.floor(room / (widest + 14)));
    const step = Math.max(1, Math.ceil(labels.length / fits));
    labels.forEach((label, index) => {
      if (index % step !== 0) return;
      const x = positions[index];
      let anchor = "middle";
      if (x - widest / 2 < 0) anchor = "start";
      else if (x + widest / 2 > area.width) anchor = "end";
      const text = svg(
        "text",
        { x, y: area.bottom + 18, "text-anchor": anchor, class: "zd-tick" },
        group,
      );
      text.textContent = label;
    });
  }

  function legend(entry, series, mark) {
    if (series.length < 2) return;
    for (const item of series) {
      const row = el("span", "zd-legend-item");
      const key = el("span", `zd-key zd-key-${mark}`);
      key.style.background = color(item.slot);
      row.append(key, el("span", "", item.label));
      entry.legend.appendChild(row);
    }
  }

  function emptyState(entry) {
    entry.body.appendChild(el("p", "zd-empty", state.copy.empty));
  }

  function frameTable(entry, frame, xFormat, valueFormatter) {
    const table = el("table", "zd-table");
    const head = table.createTHead().insertRow();
    head.appendChild(el("th", "", entry.c.xLabel || entry.c.x));
    for (const series of frame.series) head.appendChild(el("th", "zd-num", series.label));
    const body = table.createTBody();
    for (const group of frame.xs) {
      const row = body.insertRow();
      row.appendChild(el("td", "", group.folded ? group.x : xFormat(group.x)));
      for (const series of frame.series)
        row.appendChild(el("td", "zd-num", valueFormatter(group.values[series.name])));
    }
    const wrap = el("div", "zd-table-wrap");
    wrap.appendChild(table);
    entry.body.appendChild(wrap);
  }

  function renderLine(entry, filled) {
    const c = entry.c;
    const frame = buildFrame(c, rowsOf(c.dataset), rowsOf(c.dataset, false));
    sortFrame(frame, c.sort || (frame.xType === "string" ? "none" : "x"));
    if (frame.xs.length === 0) return emptyState(entry);
    const format = formatter(c.format);
    const xFormat = formatter(valueFormat(c.dataset, c.x, c.xFormat));
    legend(entry, frame.series, filled ? "rect" : "line");
    if (entry.showTable) return frameTable(entry, frame, xFormat, format);

    const stacked = Boolean(filled && c.stacked && frame.series.length > 1);
    const tops = frame.xs.map(() => 0);
    let low = Infinity;
    let high = -Infinity;
    const lines = frame.series.map((series) => {
      const points = frame.xs.map((group, index) => {
        const value = group.values[series.name];
        if (value === null || !Number.isFinite(Number(value))) return null;
        const base = stacked ? tops[index] : 0;
        if (stacked) tops[index] += Number(value);
        const point = { index, value: Number(value), base, top: base + Number(value) };
        low = Math.min(low, point.top, filled ? point.base : point.top);
        high = Math.max(high, point.top);
        return point;
      });
      return { series, points };
    });
    if (!Number.isFinite(low)) return emptyState(entry);
    if (low >= 0 && (filled || low < high * 0.5)) low = 0;
    if (high <= 0) high = 0;
    const scale = niceScale(low, high, 5, wholeNumbers(c));
    const ticks = tickFormatter(c.format);
    const labelEnds =
      !stacked && frame.series.length <= 4 && frame.xs.length > 1 && c.endLabels !== false;
    const endWidth = labelEnds
      ? Math.max(
          ...lines.map((line) => {
            const last = [...line.points].reverse().find(Boolean);
            return last ? textWidth(format(last.value), 11) : 0;
          }),
        ) + 14
      : 12;
    const area = plotArea(entry, {
      height: c.height || 260,
      yLabels: scale.ticks.map(ticks),
      right: endWidth,
    });
    drawYAxis(area, scale, ticks);
    const count = frame.xs.length;
    const xAt = (index) =>
      count === 1
        ? (area.left + area.right) / 2
        : area.left + (index * (area.right - area.left)) / (count - 1);
    const yAt = (value) => scale.map(value, area.top, area.bottom);
    drawXLabels(
      area,
      frame.xs.map((group) => xFormat(group.x)),
      frame.xs.map((_, index) => xAt(index)),
    );

    const marks = svg("g", {}, area.svg);
    for (const line of lines) {
      const stroke = color(line.series.slot);
      let path = "";
      let fill = "";
      let segment = [];
      const flush = () => {
        if (segment.length === 0) return;
        if (filled) {
          const forward = segment.map((point) => `${xAt(point.index)},${yAt(point.top)}`).join("L");
          const back = [...segment]
            .reverse()
            .map((point) => `${xAt(point.index)},${yAt(point.base)}`)
            .join("L");
          fill += `M${forward}L${back}Z`;
        }
        path += `M${segment.map((point) => `${xAt(point.index)},${yAt(point.top)}`).join("L")}`;
        segment = [];
      };
      for (const point of line.points) {
        if (point === null) flush();
        else segment.push(point);
      }
      flush();
      if (filled && fill)
        svg("path", { d: fill, class: "zd-area", style: `fill:${stroke}` }, marks);
      svg("path", { d: path, class: "zd-line", style: `stroke:${stroke}` }, marks);
      const lone = line.points.filter(Boolean);
      if (lone.length === 1) {
        svg(
          "circle",
          {
            cx: xAt(lone[0].index),
            cy: yAt(lone[0].top),
            r: 4,
            class: "zd-dot",
            style: `fill:${stroke}`,
          },
          marks,
        );
      }
    }

    if (labelEnds) {
      const ends = lines
        .map((line) => {
          const last = [...line.points].reverse().find(Boolean);
          return last ? { line, last, y: yAt(last.top) } : null;
        })
        .filter(Boolean)
        .sort((a, b) => a.y - b.y);
      const collide = ends.some((end, index) => index > 0 && end.y - ends[index - 1].y < 14);
      for (const end of ends) {
        const x = xAt(end.last.index);
        svg(
          "circle",
          { cx: x, cy: end.y, r: 4, class: "zd-dot", style: `fill:${color(end.line.series.slot)}` },
          marks,
        );
        if (!collide) {
          const label = svg("text", { x: x + 8, y: end.y + 4, class: "zd-value" }, marks);
          label.textContent = format(end.last.value);
        }
      }
    }

    const focus = svg("g", { class: "zd-focus" }, area.svg);
    const overlay = svg(
      "rect",
      {
        x: area.left,
        y: area.top,
        width: area.right - area.left,
        height: area.bottom - area.top,
        class: "zd-hit",
      },
      area.svg,
    );
    let current = count - 1;
    const show = (event, index) => {
      current = index;
      focus.replaceChildren();
      const x = xAt(index);
      svg("line", { x1: x, x2: x, y1: area.top, y2: area.bottom, class: "zd-crosshair" }, focus);
      const rows = [];
      for (const line of lines) {
        const point = line.points[index];
        if (!point) continue;
        svg(
          "circle",
          {
            cx: x,
            cy: yAt(point.top),
            r: 4,
            class: "zd-dot",
            style: `fill:${color(line.series.slot)}`,
          },
          focus,
        );
        rows.push({
          value: format(point.value),
          label: line.series.label,
          color: color(line.series.slot),
        });
      }
      const box = overlay.getBoundingClientRect();
      const anchor =
        event && event.type === "pointermove"
          ? pointOf(event, overlay)
          : { x: box.left + (x - area.left), y: box.top };
      showTip(anchor, xFormat(frame.xs[index].x), rows);
    };
    bindHover(
      overlay,
      (event) => {
        if (event.type === "focus") return show(event, current);
        const box = overlay.getBoundingClientRect();
        const relative = ((event.clientX - box.left) / (box.width || 1)) * (area.right - area.left);
        const index =
          count === 1 ? 0 : Math.round((relative / (area.right - area.left)) * (count - 1));
        show(event, Math.max(0, Math.min(count - 1, index)));
      },
      () => focus.replaceChildren(),
    );
    overlay.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      show(
        event,
        Math.max(0, Math.min(count - 1, current + (event.key === "ArrowRight" ? 1 : -1))),
      );
    });
  }

  function barPath(x, y, width, height, end, radius) {
    const w = Math.max(0, width);
    const h = Math.max(0, height);
    const r = Math.max(0, Math.min(radius, w / 2, h / 2));
    if (r === 0 || !end) return `M${x},${y}h${w}v${h}h${-w}Z`;
    if (end === "top") {
      return `M${x},${y + h}V${y + r}Q${x},${y} ${x + r},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h}Z`;
    }
    if (end === "bottom") {
      return `M${x},${y}H${x + w}V${y + h - r}Q${x + w},${y + h} ${x + w - r},${y + h}H${x + r}Q${x},${y + h} ${x},${y + h - r}Z`;
    }
    if (end === "right") {
      return `M${x},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h - r}Q${x + w},${y + h} ${x + w - r},${y + h}H${x}Z`;
    }
    return `M${x + w},${y}V${y + h}H${x + r}Q${x},${y + h} ${x},${y + h - r}V${y + r}Q${x},${y} ${x + r},${y}Z`;
  }

  function stackSpans(group, series, stacked) {
    const spans = [];
    let positive = 0;
    let negative = 0;
    for (const entry of series) {
      const value = Number(group.values[entry.name]);
      if (!Number.isFinite(value) || value === 0) continue;
      if (stacked) {
        const from = value >= 0 ? positive : negative;
        const to = from + value;
        if (value >= 0) positive = to;
        else negative = to;
        spans.push({ series: entry, value, from, to });
      } else {
        spans.push({ series: entry, value, from: 0, to: value });
      }
    }
    if (stacked) {
      const lastUp = [...spans].reverse().find((span) => span.value > 0);
      const lastDown = [...spans].reverse().find((span) => span.value < 0);
      for (const span of spans) span.outer = span === lastUp || span === lastDown;
    } else {
      for (const span of spans) span.outer = true;
    }
    return spans;
  }

  function renderBar(entry) {
    const c = entry.c;
    const frame = buildFrame(c, rowsOf(c.dataset), rowsOf(c.dataset, false));
    const ordered = frame.xType === "date" || frame.xType === "number";
    sortFrame(frame, c.sort || (ordered ? "x" : "desc"));
    foldTail(frame, c.limit);
    if (frame.xs.length === 0) return emptyState(entry);
    const format = formatter(c.format);
    const xFormat = formatter(valueFormat(c.dataset, c.x, c.xFormat));
    legend(entry, frame.series, "rect");
    if (entry.showTable) return frameTable(entry, frame, xFormat, format);

    const labels = frame.xs.map((group) => (group.folded ? group.x : xFormat(group.x)));
    const longest = Math.max(...labels.map((label) => label.length));
    const horizontal = c.orientation
      ? c.orientation === "horizontal"
      : !ordered && (frame.xs.length > 7 || longest > 12);
    const stacked = Boolean(c.stacked && frame.series.length > 1);
    let low = 0;
    let high = 0;
    const spans = frame.xs.map((group) => {
      const list = stackSpans(group, frame.series, stacked);
      for (const span of list) {
        low = Math.min(low, span.to);
        high = Math.max(high, span.to);
      }
      return list;
    });
    const scale = niceScale(low, high, 5, wholeNumbers(c));
    const ticks = tickFormatter(c.format);
    const lanes = stacked ? 1 : frame.series.length;
    const showValues = c.valueLabels !== false && lanes === 1 && !stacked;
    if (horizontal)
      return renderBarHorizontal(entry, {
        frame,
        labels,
        spans,
        scale,
        ticks,
        format,
        lanes,
        showValues,
      });

    const area = plotArea(entry, {
      height: c.height || 260,
      yLabels: scale.ticks.map(ticks),
      top: showValues ? 20 : 12,
    });
    drawYAxis(area, scale, ticks);
    const band = (area.right - area.left) / frame.xs.length;
    const thickness = Math.max(2, Math.min(BAR_MAX, (band * 0.72 - GAP * (lanes - 1)) / lanes));
    const groupWidth = thickness * lanes + GAP * (lanes - 1);
    const yAt = (value) => scale.map(value, area.top, area.bottom);
    const centers = frame.xs.map((_, index) => area.left + band * index + band / 2);
    drawXLabels(area, labels, centers);
    const marks = svg("g", {}, area.svg);
    const values = svg("g", {}, area.svg);
    frame.xs.forEach((group, index) => {
      const start = centers[index] - groupWidth / 2;
      const bars = svg("g", { class: "zd-bars" }, marks);
      for (const span of spans[index]) {
        const lane = stacked ? 0 : frame.series.indexOf(span.series);
        const x = start + lane * (thickness + GAP);
        let top = yAt(Math.max(span.from, span.to));
        let bottom = yAt(Math.min(span.from, span.to));
        if (stacked && span.from !== 0) {
          if (span.value > 0) bottom -= GAP;
          else top += GAP;
        }
        const end = span.outer ? (span.value >= 0 ? "top" : "bottom") : null;
        svg(
          "path",
          {
            d: barPath(x, top, thickness, bottom - top, end, 4),
            style: `fill:${color(span.series.slot)}`,
          },
          bars,
        );
      }
      if (
        showValues &&
        spans[index].length &&
        band >= textWidth(format(spans[index][0].value), 11) + 4
      ) {
        const span = spans[index][0];
        const y = span.value >= 0 ? yAt(span.to) - 6 : yAt(span.to) + 14;
        const text = svg(
          "text",
          { x: centers[index], y, "text-anchor": "middle", class: "zd-value" },
          values,
        );
        text.textContent = format(span.value);
      }
      const hit = svg(
        "rect",
        {
          x: area.left + band * index,
          y: area.top,
          width: band,
          height: area.bottom - area.top,
          class: "zd-hit",
        },
        area.svg,
      );
      bindHover(
        hit,
        (event) => {
          bars.classList.add("zd-lift");
          showTip(pointOf(event, hit), labels[index], tipRows(frame, group, format));
        },
        () => bars.classList.remove("zd-lift"),
      );
    });
  }

  function tipRows(frame, group, format) {
    return frame.series
      .filter((series) => group.values[series.name] !== null)
      .map((series) => ({
        value: format(group.values[series.name]),
        label: frame.series.length > 1 ? series.label : "",
        color: frame.series.length > 1 ? color(series.slot) : null,
        mark: "rect",
      }));
  }

  function renderBarHorizontal(entry, parts) {
    const { frame, labels, spans, scale, ticks, format, lanes, showValues } = parts;
    const width = widthOf(entry);
    const thickness = Math.min(BAR_MAX, lanes > 1 ? 14 : 20);
    const rowHeight = Math.max(28, thickness * lanes + GAP * (lanes - 1) + 12);
    const labelRoom = Math.min(
      width * 0.4,
      Math.max(...labels.map((label) => textWidth(label, 12))) + 4,
    );
    const valueRoom = showValues
      ? Math.max(...spans.flat().map((span) => textWidth(format(span.value), 11))) + 10
      : 12;
    const height = frame.xs.length * rowHeight + 34;
    const area = {
      width,
      height,
      left: labelRoom + 12,
      right: width - valueRoom,
      top: 6,
      bottom: height - 28,
    };
    area.svg = svg("svg", {
      class: "zd-svg",
      width,
      height,
      viewBox: `0 0 ${width} ${height}`,
      role: "img",
      "aria-label": entry.c.title || "bar",
    });
    entry.body.appendChild(area.svg);
    const xAt = linear(scale.lo, scale.hi, area.left, area.right);
    const axis = svg("g", { class: "zd-axis" }, area.svg);
    for (const value of scale.ticks) {
      const x = xAt(value);
      svg(
        "line",
        {
          x1: x,
          x2: x,
          y1: area.top,
          y2: area.bottom,
          class: value === 0 ? "zd-base" : "zd-gridline",
        },
        axis,
      );
    }
    drawXLabels(area, scale.ticks.map(ticks), scale.ticks.map(xAt));
    const marks = svg("g", {}, area.svg);
    frame.xs.forEach((group, index) => {
      const rowTop = area.top + index * rowHeight;
      const groupHeight = thickness * lanes + GAP * (lanes - 1);
      const start = rowTop + (rowHeight - groupHeight) / 2;
      const name = svg(
        "text",
        {
          x: area.left - 10,
          y: rowTop + rowHeight / 2 + 4,
          "text-anchor": "end",
          class: "zd-category",
        },
        marks,
      );
      name.textContent = truncate(labels[index], 12, labelRoom);
      const bars = svg("g", { class: "zd-bars" }, marks);
      for (const span of spans[index]) {
        const lane = lanes > 1 ? frame.series.indexOf(span.series) : 0;
        const y = start + lane * (thickness + GAP);
        let left = xAt(Math.min(span.from, span.to));
        let right = xAt(Math.max(span.from, span.to));
        if (span.from !== 0) {
          if (span.value > 0) left += GAP;
          else right -= GAP;
        }
        const end = span.outer ? (span.value >= 0 ? "right" : "left") : null;
        svg(
          "path",
          {
            d: barPath(left, y, right - left, thickness, end, 4),
            style: `fill:${color(span.series.slot)}`,
          },
          bars,
        );
      }
      if (showValues && spans[index].length) {
        const span = spans[index][0];
        const x = span.value >= 0 ? xAt(span.to) + 6 : xAt(span.to) - 6;
        const text = svg(
          "text",
          {
            x,
            y: start + thickness / 2 + 4,
            "text-anchor": span.value >= 0 ? "start" : "end",
            class: "zd-value",
          },
          marks,
        );
        text.textContent = format(span.value);
      }
      const hit = svg(
        "rect",
        { x: 0, y: rowTop, width, height: rowHeight, class: "zd-hit" },
        area.svg,
      );
      bindHover(
        hit,
        (event) => {
          bars.classList.add("zd-lift");
          showTip(pointOf(event, hit), labels[index], tipRows(frame, group, format));
        },
        () => bars.classList.remove("zd-lift"),
      );
    });
  }

  function arcPath(cx, cy, inner, outer, from, to) {
    const sweep = Math.min(to - from, Math.PI * 2 - 1e-4);
    const end = from + sweep;
    const large = sweep > Math.PI ? 1 : 0;
    const at = (radius, angle) =>
      `${cx + radius * Math.sin(angle)},${cy - radius * Math.cos(angle)}`;
    return `M${at(outer, from)}A${outer},${outer} 0 ${large} 1 ${at(outer, end)}L${at(inner, end)}A${inner},${inner} 0 ${large} 0 ${at(inner, from)}Z`;
  }

  function renderDonut(entry) {
    const c = entry.c;
    const shape = { series: c.label, y: c.value, seriesOrder: c.order };
    const names = seriesNames(
      shape,
      rowsOf(c.dataset, false),
      Math.min(c.limit || DONUT_SEGMENTS, DONUT_SEGMENTS),
    );
    const known = new Set(names);
    const buckets = new Map(names.map((name) => [name, []]));
    for (const row of rowsOf(c.dataset)) {
      let name = keyOf(row[c.label]);
      if (!known.has(name)) name = state.copy.other;
      if (buckets.has(name)) buckets.get(name).push(row);
    }
    const format = formatter(c.format);
    const segments = names
      .map((name, slot) => ({ name, slot, value: aggregate(buckets.get(name), c.value, c.agg) }))
      .filter((segment) => Number(segment.value) > 0);
    const total = segments.reduce((sum, segment) => sum + Number(segment.value), 0);
    if (segments.length === 0 || total <= 0) return emptyState(entry);
    const percent = formatter("percent");
    if (entry.showTable) {
      const frame = {
        series: [
          { name: "value", label: c.valueLabel || c.value || state.copy.total },
          { name: "share", label: "%" },
        ],
        xs: segments.map((segment) => ({
          x: segment.name,
          values: { value: segment.value, share: segment.value / total },
        })),
      };
      const table = el("table", "zd-table");
      const head = table.createTHead().insertRow();
      head.append(
        el("th", "", c.labelTitle || c.label),
        el("th", "zd-num", frame.series[0].label),
        el("th", "zd-num", "%"),
      );
      const body = table.createTBody();
      for (const segment of segments) {
        const row = body.insertRow();
        row.append(
          el("td", "", segment.name),
          el("td", "zd-num", format(segment.value)),
          el("td", "zd-num", percent(segment.value / total)),
        );
      }
      entry.body.appendChild(table);
      return;
    }
    const width = widthOf(entry);
    const size = Math.min(c.height || 220, width < 420 ? width : width * 0.5);
    const wrap = el("div", width < 420 ? "zd-donut zd-donut-stack" : "zd-donut");
    entry.body.appendChild(wrap);
    const figure = svg(
      "svg",
      {
        class: "zd-svg",
        width: size,
        height: size,
        viewBox: `0 0 ${size} ${size}`,
        role: "img",
        "aria-label": c.title || "donut",
      },
      wrap,
    );
    const outer = size / 2 - 4;
    const inner = outer * 0.62;
    let angle = 0;
    for (const segment of segments) {
      const sweep = (Number(segment.value) / total) * Math.PI * 2;
      const path = svg(
        "path",
        {
          d: arcPath(size / 2, size / 2, inner, outer, angle, angle + sweep),
          class: "zd-arc",
          style: `fill:${color(segment.slot)}`,
        },
        figure,
      );
      angle += sweep;
      bindHover(path, (event) => {
        showTip(pointOf(event, path), segment.name, [
          {
            value: format(segment.value),
            label: percent(segment.value / total),
            color: color(segment.slot),
            mark: "rect",
          },
        ]);
      });
    }
    const center = svg(
      "text",
      { x: size / 2, y: size / 2 + 2, "text-anchor": "middle", class: "zd-donut-total" },
      figure,
    );
    center.textContent = format(total);
    const caption = svg(
      "text",
      { x: size / 2, y: size / 2 + 20, "text-anchor": "middle", class: "zd-tick" },
      figure,
    );
    caption.textContent = c.centerLabel || state.copy.total;
    const list = el("ul", "zd-donut-list");
    for (const segment of segments) {
      const item = el("li", "");
      const key = el("span", "zd-key zd-key-rect");
      key.style.background = color(segment.slot);
      item.append(
        key,
        el("span", "zd-donut-name", segment.name),
        el("strong", "", format(segment.value)),
        el("span", "zd-muted", percent(segment.value / total)),
      );
      list.appendChild(item);
    }
    wrap.appendChild(list);
  }

  function renderScatter(entry) {
    const c = entry.c;
    const all = rowsOf(c.dataset, false);
    const names = c.series
      ? seriesNames({ series: c.series, y: c.y }, all, SCATTER_SERIES)
      : [c.title || ""];
    const known = new Set(names);
    const points = [];
    for (const row of rowsOf(c.dataset)) {
      const x = Number(row[c.x]);
      const y = Number(row[c.y]);
      if (isBlank(row[c.x]) || isBlank(row[c.y]) || !Number.isFinite(x) || !Number.isFinite(y))
        continue;
      let name = c.series ? keyOf(row[c.series]) : names[0];
      if (!known.has(name)) name = state.copy.other;
      if (!known.has(name)) continue;
      points.push({ x, y, name, slot: names.indexOf(name), row });
      if (points.length >= 5000) break;
    }
    const series = names.map((name, slot) => ({ name, label: name, slot }));
    legend(entry, c.series ? series : [], "rect");
    const xFormat = formatter(c.xFormat);
    const yFormat = formatter(c.format);
    if (points.length === 0) return emptyState(entry);
    if (entry.showTable) {
      const table = el("table", "zd-table");
      const head = table.createTHead().insertRow();
      if (c.label) head.appendChild(el("th", "", c.label));
      if (c.series) head.appendChild(el("th", "", c.series));
      head.append(el("th", "zd-num", c.xLabel || c.x), el("th", "zd-num", c.yLabel || c.y));
      const body = table.createTBody();
      for (const point of points.slice(0, 500)) {
        const row = body.insertRow();
        if (c.label) row.appendChild(el("td", "", point.row[c.label]));
        if (c.series) row.appendChild(el("td", "", point.name));
        row.append(el("td", "zd-num", xFormat(point.x)), el("td", "zd-num", yFormat(point.y)));
      }
      const wrap = el("div", "zd-table-wrap");
      wrap.appendChild(table);
      entry.body.appendChild(wrap);
      return;
    }
    const xs = niceScale(
      Math.min(...points.map((p) => p.x)),
      Math.max(...points.map((p) => p.x)),
      6,
    );
    const ys = niceScale(
      Math.min(...points.map((p) => p.y)),
      Math.max(...points.map((p) => p.y)),
      5,
    );
    const yTicks = tickFormatter(c.format);
    const xTicks = tickFormatter(c.xFormat);
    const area = plotArea(entry, {
      height: c.height || 280,
      yLabels: ys.ticks.map(yTicks),
      bottom: 44,
      top: 26,
    });
    drawYAxis(area, ys, yTicks);
    const xAt = linear(xs.lo, xs.hi, area.left, area.right);
    const yAt = (value) => ys.map(value, area.top, area.bottom);
    drawXLabels(area, xs.ticks.map(xTicks), xs.ticks.map(xAt));
    const xName = svg(
      "text",
      { x: area.right, y: area.bottom + 36, "text-anchor": "end", class: "zd-tick" },
      area.svg,
    );
    xName.textContent = c.xLabel || c.x;
    const yName = svg("text", { x: area.left, y: area.top - 12, class: "zd-tick" }, area.svg);
    yName.textContent = c.yLabel || c.y;
    const marks = svg("g", {}, area.svg);
    for (const point of points) {
      point.px = xAt(point.x);
      point.py = yAt(point.y);
      svg(
        "circle",
        { cx: point.px, cy: point.py, r: 4, class: "zd-dot", style: `fill:${color(point.slot)}` },
        marks,
      );
    }
    const focus = svg("g", { class: "zd-focus" }, area.svg);
    const overlay = svg(
      "rect",
      {
        x: area.left,
        y: area.top,
        width: area.right - area.left,
        height: area.bottom - area.top,
        class: "zd-hit",
      },
      area.svg,
    );
    bindHover(
      overlay,
      (event) => {
        const box = overlay.getBoundingClientRect();
        const px =
          area.left + (event.clientX - box.left) * ((area.right - area.left) / (box.width || 1));
        const py =
          area.top + (event.clientY - box.top) * ((area.bottom - area.top) / (box.height || 1));
        let best = null;
        let distance = 24 * 24;
        for (const point of points) {
          const d = (point.px - px) ** 2 + (point.py - py) ** 2;
          if (d < distance) {
            distance = d;
            best = point;
          }
        }
        focus.replaceChildren();
        if (!best) return hideTip();
        svg(
          "circle",
          { cx: best.px, cy: best.py, r: 6, class: "zd-dot", style: `fill:${color(best.slot)}` },
          focus,
        );
        const title = c.label ? String(best.row[c.label]) : c.series ? best.name : "";
        showTip(pointOf(event, overlay), title, [
          { value: xFormat(best.x), label: c.xLabel || c.x },
          { value: yFormat(best.y), label: c.yLabel || c.y },
        ]);
      },
      () => focus.replaceChildren(),
    );
  }

  function renderHeatmap(entry) {
    const c = entry.c;
    const rows = rowsOf(c.dataset);
    const cells = new Map();
    const xKeys = new Map();
    const yKeys = new Map();
    for (const row of rows) {
      const x = keyOf(row[c.x]);
      const y = keyOf(row[c.y]);
      xKeys.set(x, row[c.x]);
      yKeys.set(y, row[c.y]);
      const key = `${x}\u0000${y}`;
      if (!cells.has(key)) cells.set(key, []);
      cells.get(key).push(row);
    }
    if (cells.size === 0) return emptyState(entry);
    const order = (keys, field, explicit) => {
      const list = [...keys.entries()];
      if (Array.isArray(explicit)) {
        const fixed = explicit.map(String).filter((key) => keys.has(key));
        return fixed.concat(list.map((item) => item[0]).filter((key) => !fixed.includes(key)));
      }
      const type = typeOf(c.dataset, field);
      if (type === "string" && !c.sortCategories) return list.map((item) => item[0]);
      const compare = compareBy(type);
      return list.sort((a, b) => compare(a[1], b[1])).map((item) => item[0]);
    };
    const xs = order(xKeys, c.x, c.xOrder);
    const ys = order(yKeys, c.y, c.yOrder);
    const values = new Map();
    let low = Infinity;
    let high = -Infinity;
    for (const [key, bucket] of cells) {
      const value = aggregate(bucket, c.value, c.agg);
      values.set(key, value);
      if (Number.isFinite(Number(value)) && value !== null) {
        low = Math.min(low, Number(value));
        high = Math.max(high, Number(value));
      }
    }
    const format = formatter(c.format);
    const xFormat = formatter(valueFormat(c.dataset, c.x, c.xFormat));
    const yFormat = formatter(valueFormat(c.dataset, c.y, c.yFormat));
    if (entry.showTable) {
      const table = el("table", "zd-table");
      const head = table.createTHead().insertRow();
      head.appendChild(el("th", "", ""));
      for (const x of xs) head.appendChild(el("th", "zd-num", xFormat(xKeys.get(x))));
      const body = table.createTBody();
      for (const y of ys) {
        const row = body.insertRow();
        row.appendChild(el("th", "", yFormat(yKeys.get(y))));
        for (const x of xs)
          row.appendChild(el("td", "zd-num", format(values.get(`${x}\u0000${y}`))));
      }
      const wrap = el("div", "zd-table-wrap");
      wrap.appendChild(table);
      entry.body.appendChild(wrap);
      return;
    }
    const width = widthOf(entry);
    const yLabels = ys.map((y) => yFormat(yKeys.get(y)));
    const left =
      Math.min(width * 0.3, Math.max(...yLabels.map((label) => textWidth(label, 11)))) + 12;
    const cellWidth = (width - left - 4) / xs.length;
    const cellHeight = c.cellHeight || 28;
    const height = ys.length * cellHeight + 30;
    const figure = svg("svg", {
      class: "zd-svg",
      width,
      height,
      viewBox: `0 0 ${width} ${height}`,
      role: "img",
      "aria-label": c.title || "heatmap",
    });
    entry.body.appendChild(figure);
    const step = (value) =>
      high === low
        ? Math.floor(SEQUENTIAL.length / 2)
        : Math.round(((value - low) / (high - low)) * (SEQUENTIAL.length - 1));
    ys.forEach((y, row) => {
      const label = svg(
        "text",
        {
          x: left - 8,
          y: row * cellHeight + cellHeight / 2 + 4,
          "text-anchor": "end",
          class: "zd-tick",
        },
        figure,
      );
      label.textContent = truncate(yLabels[row], 11, left - 10);
      xs.forEach((x, column) => {
        const value = values.get(`${x}\u0000${y}`);
        const cx = left + column * cellWidth;
        const cy = row * cellHeight;
        const present = value !== null && value !== undefined && Number.isFinite(Number(value));
        const index = present ? step(Number(value)) : -1;
        const cell = svg(
          "rect",
          {
            x: cx + GAP / 2,
            y: cy + GAP / 2,
            width: Math.max(1, cellWidth - GAP),
            height: cellHeight - GAP,
            rx: 3,
            class: present ? "zd-cell" : "zd-cell zd-cell-empty",
            style: present ? `fill:${SEQUENTIAL[index]}` : undefined,
          },
          figure,
        );
        if (present && c.cellLabels !== false) {
          const text = format(value);
          if (textWidth(text, 10) + 8 <= cellWidth && cellHeight >= 18) {
            const label = svg(
              "text",
              {
                x: cx + cellWidth / 2,
                y: cy + cellHeight / 2 + 4,
                "text-anchor": "middle",
                class: "zd-cell-label",
                style: `fill:${index >= DARK_LABEL_FROM ? "#ffffff" : "#0b0b0b"}`,
              },
              figure,
            );
            label.textContent = text;
          }
        }
        bindHover(cell, (event) => {
          showTip(pointOf(event, cell), `${yLabels[row]} · ${xFormat(xKeys.get(x))}`, [
            { value: format(value) },
          ]);
        });
      });
    });
    const xLabels = xs.map((x) => xFormat(xKeys.get(x)));
    drawXLabels(
      { svg: figure, left, right: width, width, bottom: ys.length * cellHeight - 4 },
      xLabels,
      xs.map((_, column) => left + column * cellWidth + cellWidth / 2),
    );
    const scale = el("div", "zd-scale");
    scale.appendChild(el("span", "zd-muted", format(low)));
    const ramp = el("span", "zd-ramp");
    ramp.style.background = `linear-gradient(90deg, ${SEQUENTIAL.join(", ")})`;
    scale.appendChild(ramp);
    scale.appendChild(el("span", "zd-muted", format(high)));
    entry.legend.appendChild(scale);
  }

  function renderFunnel(entry) {
    const c = entry.c;
    const groups = new Map();
    for (const row of rowsOf(c.dataset)) {
      const key = keyOf(row[c.label]);
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(row);
    }
    let names = [...groups.keys()];
    if (Array.isArray(c.order)) {
      const fixed = c.order.map(String).filter((name) => groups.has(name));
      names = fixed.concat(names.filter((name) => !fixed.includes(name)));
    }
    const stages = names.map((name) => ({
      name,
      value: Number(aggregate(groups.get(name), c.value, c.agg)) || 0,
    }));
    if (stages.length === 0) return emptyState(entry);
    const format = formatter(c.format);
    const percent = formatter("percent");
    const first = stages[0].value || 1;
    if (entry.showTable) {
      const table = el("table", "zd-table");
      const head = table.createTHead().insertRow();
      head.append(
        el("th", "", c.labelTitle || c.label),
        el("th", "zd-num", c.valueLabel || c.value || ""),
        el("th", "zd-num", state.copy.ofFirst),
        el("th", "zd-num", state.copy.fromPrevious),
      );
      const body = table.createTBody();
      stages.forEach((stage, index) => {
        const row = body.insertRow();
        row.append(
          el("td", "", stage.name),
          el("td", "zd-num", format(stage.value)),
          el("td", "zd-num", percent(stage.value / first)),
          el(
            "td",
            "zd-num",
            index ? percent(stage.value / (stages[index - 1].value || 1)) : state.copy.noValue,
          ),
        );
      });
      entry.body.appendChild(table);
      return;
    }
    const list = el("div", "zd-funnel");
    stages.forEach((stage, index) => {
      const row = el("div", "zd-funnel-row");
      const head = el("div", "zd-funnel-head");
      head.append(el("span", "zd-funnel-name", stage.name), el("strong", "", format(stage.value)));
      const track = el("div", "zd-funnel-track");
      const bar = el("div", "zd-funnel-bar");
      const tone =
        stages.length === 1
          ? ORDINAL.length - 1
          : Math.round((1 - index / (stages.length - 1)) * (ORDINAL.length - 1));
      bar.style.background = ORDINAL[tone];
      bar.style.width = `${Math.max(0.5, (stage.value / first) * 100)}%`;
      track.appendChild(bar);
      const meta = el(
        "div",
        "zd-muted zd-funnel-meta",
        index === 0
          ? ""
          : `${percent(stage.value / first)} ${state.copy.ofFirst} · ${percent(stage.value / (stages[index - 1].value || 1))} ${state.copy.fromPrevious}`,
      );
      row.append(head, track, meta);
      list.appendChild(row);
    });
    entry.body.appendChild(list);
  }

  function renderKpi(entry) {
    const c = entry.c;
    const rows = c.dataset ? rowsOf(c.dataset) : [];
    const value = measure(rows, c.value, c.agg, c.dataset);
    const format = formatter(c.format);
    entry.body.classList.add("zd-kpi");
    if (c.hero) entry.body.classList.add("zd-hero");
    entry.body.appendChild(el("div", "zd-kpi-value", format(value)));
    if (c.compare !== undefined && value !== null) {
      const base = measure(rows, c.compare, c.agg, c.dataset);
      if (base !== null && Number.isFinite(Number(base))) {
        const change = Number(value) - Number(base);
        const relative = Number(base) === 0 ? null : change / Math.abs(Number(base));
        const direction = change > 0 ? "up" : change < 0 ? "down" : "flat";
        const good =
          direction === "flat" ? null : (direction === "up") === ((c.goodWhen || "up") === "up");
        const delta = el(
          "div",
          `zd-delta ${good === null ? "zd-flat" : good ? "zd-good" : "zd-bad"}`,
        );
        const chip = el("span", "zd-delta-chip");
        chip.appendChild(
          el("span", "zd-delta-icon", direction === "up" ? "▲" : direction === "down" ? "▼" : "●"),
        );
        const shown =
          c.deltaMode === "absolute" || relative === null
            ? `${change > 0 ? "+" : ""}${format(change)}`
            : `${relative > 0 ? "+" : ""}${formatter("percent")(relative)}`;
        chip.appendChild(el("span", "", shown));
        delta.appendChild(chip);
        if (c.compareLabel) delta.appendChild(el("span", "zd-muted", c.compareLabel));
        entry.body.appendChild(delta);
      }
    }
    if (c.spark && c.dataset) renderSpark(entry, rows);
  }

  function renderSpark(entry, rows) {
    const c = entry.c;
    const frame = buildFrame(
      { dataset: c.dataset, x: c.spark.x, y: c.spark.y, agg: c.spark.agg },
      rows,
      rows,
    );
    sortFrame(frame, "x");
    const name = frame.series[0].name;
    const values = frame.xs
      .slice(-(c.spark.points || 12))
      .map((group) => Number(group.values[name]));
    const finite = values.filter(Number.isFinite);
    if (finite.length < 2) return;
    const width = Math.min(widthOf(entry), 220);
    const height = 36;
    const low = Math.min(...finite);
    const high = Math.max(...finite);
    const xAt = (index) => 4 + (index * (width - 8)) / (values.length - 1);
    const yAt = (value) =>
      high === low ? height / 2 : 4 + ((high - value) / (high - low)) * (height - 8);
    const figure = svg("svg", {
      class: "zd-spark",
      width,
      height,
      viewBox: `0 0 ${width} ${height}`,
      "aria-hidden": "true",
    });
    const points = values
      .map((value, index) => (Number.isFinite(value) ? `${xAt(index)},${yAt(value)}` : null))
      .filter(Boolean);
    svg("path", { d: `M${points.join("L")}`, class: "zd-spark-line" }, figure);
    const last = values.length - 1;
    if (Number.isFinite(values[last]))
      svg(
        "circle",
        { cx: xAt(last), cy: yAt(values[last]), r: 3.5, class: "zd-dot", style: "fill:var(--s1)" },
        figure,
      );
    entry.body.appendChild(figure);
  }

  function renderMeter(entry) {
    const c = entry.c;
    const rows = c.dataset ? rowsOf(c.dataset) : [];
    const value = Number(measure(rows, c.value, c.agg, c.dataset));
    const target = Number(measure(rows, c.target, c.agg, c.dataset));
    const format = formatter(c.format);
    const ratio = target ? value / target : 0;
    const head = el("div", "zd-meter-head");
    head.append(
      el("strong", "zd-meter-value", format(value)),
      el(
        "span",
        "zd-muted",
        `${state.copy.target}: ${format(target)} · ${formatter("percent")(ratio)}`,
      ),
    );
    const track = el("div", "zd-meter-track");
    track.setAttribute("role", "meter");
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", String(target));
    track.setAttribute("aria-valuenow", String(value));
    const fill = el("div", "zd-meter-fill");
    fill.style.width = `${Math.max(0, Math.min(1, ratio)) * 100}%`;
    track.appendChild(fill);
    entry.body.append(head, track);
  }

  function columnsOf(c) {
    const data = state.data[c.dataset];
    const list = c.columns || data.columns.map((column) => column.name);
    return list.map((column) => {
      const spec = typeof column === "string" ? { field: column } : column;
      const type = data.types[spec.field] || "string";
      return {
        field: spec.field,
        label: spec.label || spec.field,
        type,
        bar: Boolean(spec.bar),
        format: formatter(spec.format || valueFormat(c.dataset, spec.field)),
      };
    });
  }

  function renderTable(entry) {
    const c = entry.c;
    entry.table = entry.table || { sort: c.sort || null, query: "", expanded: false };
    if (c.search !== false && rowsOf(c.dataset).length > 10) {
      const search = searchField(state.copy.search, entry.table.query, (text) => {
        entry.table.query = text;
        drawTable(entry, host);
      });
      search.node.classList.add("zd-search");
      entry.body.appendChild(search.node);
    }
    const host = el("div", "zd-table-host");
    entry.body.appendChild(host);
    drawTable(entry, host);
  }

  function drawTable(entry, host) {
    const c = entry.c;
    const columns = columnsOf(c);
    const view = entry.table;
    let rows = rowsOf(c.dataset);
    const query = view.query.trim().toLowerCase();
    if (query) {
      rows = rows.filter((row) =>
        columns.some((column) =>
          String(row[column.field] ?? "")
            .toLowerCase()
            .includes(query),
        ),
      );
    }
    if (view.sort) {
      const column = columns.find((item) => item.field === view.sort.field);
      const compare = compareBy(column ? column.type : "string");
      const sign = view.sort.dir === "asc" ? 1 : -1;
      rows = [...rows].sort((a, b) => {
        const left = a[view.sort.field];
        const right = b[view.sort.field];
        if (isBlank(left)) return 1;
        if (isBlank(right)) return -1;
        return sign * compare(left, right);
      });
    }
    const limit = c.limit || 50;
    const shown = view.expanded ? rows : rows.slice(0, limit);
    const maxima = {};
    for (const column of columns) {
      if (column.bar)
        maxima[column.field] = Math.max(
          1e-9,
          ...rows.map((row) => Math.abs(Number(row[column.field]) || 0)),
        );
    }
    const table = el("table", "zd-table");
    const head = table.createTHead().insertRow();
    for (const column of columns) {
      const cell = el("th", column.type === "number" ? "zd-num" : "");
      const sorter = el("button", "zd-sort", column.label);
      sorter.type = "button";
      const active = view.sort && view.sort.field === column.field;
      sorter.dataset.active = String(Boolean(active));
      if (active)
        cell.setAttribute("aria-sort", view.sort.dir === "asc" ? "ascending" : "descending");
      if (active)
        sorter.appendChild(el("span", "zd-sort-mark", view.sort.dir === "asc" ? "↑" : "↓"));
      sorter.addEventListener("click", () => {
        const dir = active && view.sort.dir === "desc" ? "asc" : "desc";
        view.sort = { field: column.field, dir };
        drawTable(entry, host);
      });
      cell.appendChild(sorter);
      head.appendChild(cell);
    }
    const body = table.createTBody();
    for (const row of shown) {
      const line = body.insertRow();
      for (const column of columns) {
        const value = row[column.field];
        const cell = el("td", column.type === "number" ? "zd-num" : "");
        if (column.bar && !isBlank(value)) {
          const bar = el("span", "zd-cell-bar");
          bar.style.width = `${(Math.abs(Number(value)) / maxima[column.field]) * 100}%`;
          cell.classList.add("zd-with-bar");
          cell.append(bar, el("span", "zd-cell-text", column.format(value)));
        } else {
          cell.textContent = column.format(value);
        }
        line.appendChild(cell);
      }
    }
    const wrap = el("div", "zd-table-wrap");
    wrap.appendChild(table);
    const footer = el("div", "zd-table-footer");
    if (rows.length === 0) footer.appendChild(el("span", "zd-muted", state.copy.empty));
    else footer.appendChild(el("span", "zd-muted", state.copy.shown(shown.length, rows.length)));
    if (shown.length < rows.length) {
      const more = button(state.copy.showAll, "ghost", "sm");
      more.addEventListener("click", () => {
        view.expanded = true;
        drawTable(entry, host);
      });
      footer.appendChild(more);
    }
    host.replaceChildren(wrap, footer);
  }

  function renderText(entry) {
    const blocks = String(entry.c.text || "").split(/\n{2,}/);
    for (const block of blocks) {
      const lines = block.split("\n");
      if (lines.every((line) => /^\s*[-•]\s+/.test(line))) {
        const list = el("ul", "zd-text-list");
        for (const line of lines)
          list.appendChild(inline(el("li"), line.replace(/^\s*[-•]\s+/, "")));
        entry.body.appendChild(list);
      } else {
        entry.body.appendChild(inline(el("p", "zd-text"), lines.join("\n")));
      }
    }
  }

  function inline(node, text) {
    const parts = String(text).split(/(\*\*[^*]+\*\*)/);
    for (const part of parts) {
      if (/^\*\*[^*]+\*\*$/.test(part)) node.appendChild(el("strong", "", part.slice(2, -2)));
      else if (part) node.appendChild(document.createTextNode(part));
    }
    return node;
  }

  const RENDERERS = {
    kpi: renderKpi,
    meter: renderMeter,
    line: (entry) => renderLine(entry, false),
    area: (entry) => renderLine(entry, true),
    bar: renderBar,
    donut: renderDonut,
    scatter: renderScatter,
    heatmap: renderHeatmap,
    funnel: renderFunnel,
    table: renderTable,
    text: renderText,
  };

  function renderCard(entry) {
    entry.body.replaceChildren();
    entry.legend.replaceChildren();
    entry.body.className = "zd-body";
    const renderer = RENDERERS[entry.c.type];
    try {
      if (!renderer) throw new Error(`unknown type "${entry.c.type}"`);
      renderer(entry);
    } catch (error) {
      entry.body.replaceChildren(el("p", "zd-error", `${state.copy.failed}: ${error.message}`));
      if (window.console) console.error(error);
    }
    if (entry.toggle) entry.toggle.set(entry.showTable ? "table" : "chart");
  }

  function buildCard(c, columns) {
    const span = Math.max(
      1,
      Math.min(
        columns,
        c.span || (c.type === "kpi" || c.type === "meter" ? 1 : columns >= 4 ? 2 : columns),
      ),
    );
    const node = el("article", `zd-card zd-card-${c.type}`);
    node.dataset.span = String(span);
    node.style.gridColumn = `span ${span}`;
    const entry = { c, node, showTable: false, width: 0 };
    const header = el("header", "zd-card-head");
    const titles = el("div", "zd-card-titles");
    if (c.title || c.label) titles.appendChild(el("h3", "zd-card-title", c.title || c.label));
    if (c.subtitle) titles.appendChild(el("p", "zd-card-subtitle", c.subtitle));
    header.appendChild(titles);
    if (CHART_TYPES.includes(c.type)) {
      entry.toggle = segmented(
        [
          { value: "chart", label: state.copy.chart },
          { value: "table", label: state.copy.table },
        ],
        "chart",
        state.copy.view,
        (mode) => {
          entry.showTable = mode === "table";
          hideTip();
          renderCard(entry);
        },
        true,
      );
      entry.toggle.node.classList.add("zd-toggle");
      header.appendChild(entry.toggle.node);
    }
    node.appendChild(header);
    entry.legend = el("div", "zd-legend");
    node.appendChild(entry.legend);
    entry.body = el("div", "zd-body");
    node.appendChild(entry.body);
    if (c.note) node.appendChild(el("p", "zd-note", c.note));
    return entry;
  }

  function filterOptions(filter) {
    const values = new Map();
    let type = "string";
    for (const [name, data] of Object.entries(state.data)) {
      if (!filterApplies(filter, name)) continue;
      type = data.types[filter.field] || type;
      for (const row of data.rows)
        if (!isBlank(row[filter.field])) values.set(String(row[filter.field]), row[filter.field]);
    }
    const compare = compareBy(type);
    const list = [...values.values()].sort(compare);
    return { list, type };
  }

  function renderFilters(root) {
    const filters = state.spec.filters || [];
    if (filters.length === 0) return;
    const bar = el("div", "zd-filters");
    bar.setAttribute("role", "group");
    const reset = button(state.copy.reset, "ghost", "sm");
    reset.classList.add("zd-filter-reset");
    const sync = () => {
      reset.hidden = !filters.some(filterActive);
    };
    const changed = () => {
      sync();
      renderAll();
    };
    filters.forEach((filter, index) => {
      filter.id = filter.id || `f${index}`;
      const name = filter.label || filter.field;
      const field = el("div", "zd-filter");
      field.appendChild(el("span", "zd-filter-label", name));
      const controls = el("div", "zd-filter-controls");
      const { list, type } = filterOptions(filter);
      if (filter.type === "dateRange") {
        const days = list
          .map(dayOf)
          .filter((day) => /^\d{4}-\d{2}-\d{2}$/.test(day))
          .sort();
        const bounds = days.length ? { min: days[0], max: days[days.length - 1] } : {};
        const value = state.filters[filter.id] || { from: filter.from || "", to: filter.to || "" };
        state.filters[filter.id] = value;
        const from = datePicker({
          ...bounds,
          value: value.from,
          label: `${name}: ${state.copy.from}`,
          placeholder: state.copy.from,
          onChange: (day) => {
            state.filters[filter.id] = { ...state.filters[filter.id], from: day };
            changed();
          },
        });
        const to = datePicker({
          ...bounds,
          value: value.to,
          label: `${name}: ${state.copy.to}`,
          placeholder: state.copy.to,
          onChange: (day) => {
            state.filters[filter.id] = { ...state.filters[filter.id], to: day };
            changed();
          },
        });
        controls.append(from.node, el("span", "zd-muted", "—"), to.node);
        filter.controls = [from, to];
      } else {
        const format = formatter(
          filter.format || (type === "date" ? "date" : type === "number" ? "auto" : "text"),
        );
        if (!isBlank(filter.default)) state.filters[filter.id] = String(filter.default);
        const choice = select({
          label: name,
          value: state.filters[filter.id] || "",
          options: [
            { value: "", label: state.copy.all },
            ...list.map((value) => ({ value: String(value), label: format(value) })),
          ],
          onChange: (value) => {
            state.filters[filter.id] = value;
            changed();
          },
        });
        controls.appendChild(choice.node);
        filter.controls = [choice];
      }
      field.appendChild(controls);
      bar.appendChild(field);
    });
    reset.addEventListener("click", () => {
      for (const filter of filters) {
        state.filters[filter.id] = filter.type === "dateRange" ? { from: "", to: "" } : "";
        for (const control of filter.controls) control.set("");
      }
      changed();
    });
    bar.appendChild(reset);
    sync();
    root.appendChild(bar);
  }

  const THEMES = ["auto", "light", "dark"];

  function applyTheme(mode) {
    const root = document.documentElement;
    if (mode === "auto") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", mode);
  }

  function renderHeader(root) {
    const spec = state.spec;
    const header = el("header", "zd-header");
    const titles = el("div", "zd-titles");
    titles.appendChild(el("h1", "zd-title", spec.title || ""));
    if (spec.subtitle) titles.appendChild(el("p", "zd-subtitle", spec.subtitle));
    const meta = [];
    if (spec.generatedAt)
      meta.push(`${state.copy.generated}: ${formatter("datetime")(spec.generatedAt)}`);
    if (spec.period) meta.push(spec.period);
    if (meta.length) titles.appendChild(el("p", "zd-meta", meta.join(" · ")));
    header.appendChild(titles);
    let mode = "auto";
    try {
      mode = window.localStorage.getItem("zvs-dashboard-theme") || spec.theme || "auto";
    } catch {
      mode = spec.theme || "auto";
    }
    if (!THEMES.includes(mode)) mode = "auto";
    applyTheme(mode);
    const toggle = segmented(
      THEMES.map((theme) => ({ value: theme, label: state.copy.themes[theme] })),
      mode,
      state.copy.theme,
      (next) => {
        mode = next;
        applyTheme(mode);
        try {
          window.localStorage.setItem("zvs-dashboard-theme", mode);
        } catch {
          return;
        }
      },
    );
    toggle.node.classList.add("zd-theme");
    header.appendChild(toggle.node);
    root.appendChild(header);
  }

  function renderSections(root) {
    for (const section of state.spec.sections || []) {
      const node = el("section", "zd-section");
      const columns = Math.max(1, Math.min(4, section.columns || 4));
      if (section.title) node.appendChild(el("h2", "zd-section-title", section.title));
      if (section.description)
        node.appendChild(el("p", "zd-section-description", section.description));
      const grid = el("div", "zd-grid");
      grid.dataset.cols = String(columns);
      for (const component of section.items || []) {
        const entry = buildCard(component, columns);
        grid.appendChild(entry.node);
        state.cards.push(entry);
      }
      node.appendChild(grid);
      root.appendChild(node);
    }
  }

  function renderFooter(root) {
    const spec = state.spec;
    const notes = spec.notes || [];
    const sources = spec.sources || [];
    if (!notes.length && !sources.length) return;
    const footer = el("footer", "zd-footer");
    for (const note of notes) footer.appendChild(inline(el("p", "zd-note"), note));
    if (sources.length)
      footer.appendChild(el("p", "zd-note", `${state.copy.sources}: ${sources.join("; ")}`));
    root.appendChild(footer);
  }

  function renderAll() {
    hideTip();
    for (const entry of state.cards) renderCard(entry);
  }

  function watchSizes() {
    if (typeof ResizeObserver !== "function") return;
    let pending = null;
    const observer = new ResizeObserver((records) => {
      const changed = new Set();
      for (const record of records) {
        const entry = state.cards.find((card) => card.body === record.target);
        if (!entry) continue;
        const width = Math.round(record.contentRect.width);
        if (width && width !== entry.width) {
          entry.width = width;
          changed.add(entry);
        }
      }
      if (changed.size === 0) return;
      if (pending) cancelAnimationFrame(pending);
      pending = requestAnimationFrame(() => {
        pending = null;
        for (const entry of changed) renderCard(entry);
      });
    });
    for (const entry of state.cards) {
      if (CHART_TYPES.includes(entry.c.type) || entry.c.spark) {
        entry.width = Math.round(entry.body.clientWidth);
        observer.observe(entry.body);
      }
    }
  }

  function start() {
    const root = document.getElementById("app");
    try {
      load();
    } catch (error) {
      root.appendChild(el("p", "zd-error", `${COPY.ru.failed}: ${error.message}`));
      return;
    }
    document.documentElement.lang = state.spec.lang || "ru";
    renderHeader(root);
    renderFilters(root);
    const main = el("main", "zd-main");
    root.appendChild(main);
    renderSections(main);
    renderFooter(root);
    renderAll();
    watchSizes();
    window.addEventListener("scroll", hideTip, { passive: true });
    document.documentElement.dataset.ready = "true";
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
