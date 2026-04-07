window.initializeTagPicker = function () {
  const picker = document.querySelector(".js-tag-picker");
  if (!picker) {
    return;
  }

  const input = picker.querySelector(".js-tag-picker-input");
  const hiddenInputs = picker.querySelector(".js-tag-picker-hidden-inputs");
  const selectedList = picker.querySelector(".js-tag-picker-selected");
  const menu = picker.querySelector(".js-tag-picker-menu");
  const searchUrl = picker.getAttribute("data-tag-search-url");

  if (!input || !hiddenInputs || !selectedList || !menu || !searchUrl) {
    return;
  }

  let suggestions = [];
  let selectedIndex = -1;
  let debounceId = null;
  let requestToken = 0;

  const selectedIds = new Set();
  selectedList.querySelectorAll("[data-tag-id]").forEach((item) => {
    selectedIds.add(item.getAttribute("data-tag-id"));
  });

  const closeMenu = () => {
    menu.classList.remove("is-open");
    menu.innerHTML = "";
    suggestions = [];
    selectedIndex = -1;
  };

  const syncOptionState = () => {
    menu.querySelectorAll(".action-search-option").forEach((option) => {
      const optionIndex = Number(option.dataset.index);
      option.classList.toggle("is-selected", optionIndex === selectedIndex);
    });
  };

  const removeTag = (tagId) => {
    selectedIds.delete(tagId);

    const pill = selectedList.querySelector(`[data-tag-id="${tagId}"]`);
    if (pill) {
      pill.remove();
    }

    const inputEl = hiddenInputs.querySelector(`[data-tag-id="${tagId}"]`);
    if (inputEl) {
      inputEl.remove();
    }
  };

  const addTag = (tag) => {
    const tagId = String(tag.id);
    if (selectedIds.has(tagId)) {
      input.value = "";
      closeMenu();
      return;
    }

    selectedIds.add(tagId);

    const hiddenInput = document.createElement("input");
    hiddenInput.type = "checkbox";
    hiddenInput.name = "tag_ids";
    hiddenInput.value = tagId;
    hiddenInput.checked = true;
    hiddenInput.hidden = true;
    hiddenInput.dataset.tagId = tagId;
    hiddenInputs.appendChild(hiddenInput);

    const pill = document.createElement("span");
    pill.className = "tag-badge tag-picker-pill";
    pill.setAttribute("style", tag.color_style);
    pill.dataset.tagId = tagId;

    const label = document.createElement("span");
    label.textContent = tag.name;
    pill.appendChild(label);

    const removeButton = document.createElement("button");
    removeButton.type = "button";
    removeButton.className = "tag-pill-remove";
    removeButton.setAttribute("aria-label", `Remove tag ${tag.name}`);
    removeButton.textContent = "x";
    removeButton.addEventListener("click", function () {
      removeTag(tagId);
    });
    pill.appendChild(removeButton);

    selectedList.appendChild(pill);
    input.value = "";
    input.focus();
    closeMenu();
  };

  const renderSuggestions = (items) => {
    suggestions = items.filter((item) => !selectedIds.has(String(item.id)));
    selectedIndex = -1;
    menu.innerHTML = "";

    if (!suggestions.length) {
      closeMenu();
      return;
    }

    suggestions.forEach((item, index) => {
      const option = document.createElement("li");
      option.className = "action-search-option";
      option.setAttribute("role", "option");
      option.dataset.index = String(index);

      const badge = document.createElement("span");
      badge.className = "tag-badge";
      badge.setAttribute("style", item.color_style);
      badge.textContent = item.name;
      option.appendChild(badge);

      option.addEventListener("mousedown", function (event) {
        event.preventDefault();
        addTag(item);
      });

      menu.appendChild(option);
    });

    menu.classList.add("is-open");
  };

  const search = async () => {
    const query = input.value.trim();
    const token = ++requestToken;

    try {
      const response = await fetch(`${searchUrl}?q=${encodeURIComponent(query)}`);
      if (!response.ok) {
        closeMenu();
        return;
      }

      const items = await response.json();
      if (token !== requestToken) {
        return;
      }

      renderSuggestions(Array.isArray(items) ? items : []);
    } catch (error) {
      closeMenu();
    }
  };

  document.addEventListener("click", function (event) {
    if (!event.target.closest(".js-tag-picker")) {
      closeMenu();
    }
  });

  input.addEventListener("input", function () {
    if (debounceId) {
      window.clearTimeout(debounceId);
    }
    debounceId = window.setTimeout(search, 150);
  });

  input.addEventListener("focus", function () {
    search();
  });

  input.addEventListener("keydown", function (event) {
    const isOpen = menu.classList.contains("is-open");

    if (event.key === "Backspace" && !input.value.trim()) {
      const lastPill = selectedList.querySelector(".tag-picker-pill:last-child");
      if (lastPill) {
        removeTag(lastPill.dataset.tagId);
      }
      return;
    }

    if (!isOpen) {
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      selectedIndex = (selectedIndex + 1 + suggestions.length) % suggestions.length;
      syncOptionState();
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      selectedIndex = (selectedIndex - 1 + suggestions.length) % suggestions.length;
      syncOptionState();
      return;
    }

    if (event.key === "Enter" && selectedIndex >= 0 && suggestions[selectedIndex]) {
      event.preventDefault();
      addTag(suggestions[selectedIndex]);
      return;
    }

    if (event.key === "Escape") {
      closeMenu();
    }
  });

  selectedList.querySelectorAll(".tag-pill-remove").forEach((button) => {
    button.addEventListener("click", function () {
      const pill = button.closest("[data-tag-id]");
      if (pill) {
        removeTag(pill.dataset.tagId);
      }
    });
  });
};
