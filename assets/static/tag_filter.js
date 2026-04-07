window.initializeTagFilter = function () {
  const filter = document.querySelector(".js-tag-filter");
  if (!filter) {
    return;
  }

  const input = filter.querySelector(".js-tag-filter-input");
  const hiddenInput = filter.querySelector(".js-tag-filter-hidden");
  const selectedWrap = filter.querySelector(".js-tag-filter-selected");
  const menu = filter.querySelector(".js-tag-filter-menu");
  const searchUrl = filter.getAttribute("data-tag-search-url");

  if (!input || !hiddenInput || !selectedWrap || !menu || !searchUrl) {
    return;
  }

  let suggestions = [];
  let selectedIndex = -1;
  let debounceId = null;
  let requestToken = 0;

  const closeMenu = () => {
    menu.classList.remove("is-open");
    menu.innerHTML = "";
    suggestions = [];
    selectedIndex = -1;
  };

  const setSelectedIndex = (nextIndex) => {
    if (!suggestions.length) {
      selectedIndex = -1;
      return;
    }

    selectedIndex = (nextIndex + suggestions.length) % suggestions.length;
    menu.querySelectorAll(".action-search-option").forEach((option) => {
      option.classList.toggle("is-selected", Number(option.dataset.index) === selectedIndex);
    });
  };

  const clearSelection = () => {
    hiddenInput.value = "";
    selectedWrap.innerHTML = "";
  };

  const selectTag = (tag) => {
    hiddenInput.value = tag.id;
    selectedWrap.innerHTML = `
      <span class="tag-badge tag-filter-pill" style="${tag.color_style}" data-tag-id="${tag.id}">
        <span>${tag.name}</span>
        <button type="button" class="tag-pill-remove js-tag-filter-clear" aria-label="Clear tag filter">x</button>
      </span>
    `;

    const clearButton = selectedWrap.querySelector(".js-tag-filter-clear");
    if (clearButton) {
      clearButton.addEventListener("click", function () {
        clearSelection();
      });
    }

    input.value = "";
    closeMenu();
  };

  const renderSuggestions = (items) => {
    const selectedTagId = hiddenInput.value;
    suggestions = items.filter((item) => String(item.id) !== selectedTagId);
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
        selectTag(item);
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
    if (!event.target.closest(".js-tag-filter")) {
      closeMenu();
    }
  });

  const clearButton = selectedWrap.querySelector(".js-tag-filter-clear");
  if (clearButton) {
    clearButton.addEventListener("click", function () {
      clearSelection();
    });
  }

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

    if (event.key === "Backspace" && !input.value.trim() && hiddenInput.value) {
      clearSelection();
      return;
    }

    if (!isOpen) {
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelectedIndex(selectedIndex + 1);
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelectedIndex(selectedIndex - 1);
      return;
    }

    if (event.key === "Enter" && selectedIndex >= 0 && suggestions[selectedIndex]) {
      event.preventDefault();
      selectTag(suggestions[selectedIndex]);
      return;
    }

    if (event.key === "Escape") {
      closeMenu();
    }
  });
};
