window.initializeActionItemSearch = function () {
  const table = document.getElementById("items");
  if (!table) {
    return null;
  }

  const searchUrl = table.getAttribute("data-action-search-url");
  if (!searchUrl) {
    return null;
  }

  let activeMenu = null;

  const closeMenu = (menu) => {
    if (!menu) {
      return;
    }
    menu.classList.remove("is-open");
    menu.innerHTML = "";
    if (activeMenu === menu) {
      activeMenu = null;
    }
  };

  const closeAllMenus = () => {
    table.querySelectorAll(".action-search-menu").forEach((menu) => closeMenu(menu));
  };

  document.addEventListener("click", function (event) {
    if (event.target.closest(".autocomplete-cell")) {
      return;
    }
    closeAllMenus();
  });

  const bindInput = (input) => {
    if (!input || input.dataset.searchBound === "true") {
      return;
    }

    input.dataset.searchBound = "true";
    const cell = input.closest("td");
    if (!cell) {
      return;
    }

    cell.classList.add("autocomplete-cell");
    const menu = document.createElement("ul");
    menu.className = "action-search-menu";
    menu.setAttribute("role", "listbox");
    cell.appendChild(menu);

    let selectedIndex = -1;
    let suggestions = [];
    let debounceId = null;
    let requestToken = 0;

    const selectSuggestion = (value) => {
      input.value = value;
      closeMenu(menu);
    };

    const renderSuggestions = (items) => {
      suggestions = items;
      selectedIndex = -1;
      menu.innerHTML = "";

      if (!items.length) {
        closeMenu(menu);
        return;
      }

      items.forEach((item, index) => {
        const option = document.createElement("li");
        option.className = "action-search-option";
        option.textContent = item.name;
        option.setAttribute("role", "option");
        option.addEventListener("mousedown", function (event) {
          event.preventDefault();
          selectSuggestion(item.name);
        });
        option.dataset.index = String(index);
        menu.appendChild(option);
      });

      if (activeMenu && activeMenu !== menu) {
        closeMenu(activeMenu);
      }
      menu.classList.add("is-open");
      activeMenu = menu;
    };

    const setSelectedIndex = (nextIndex) => {
      if (!suggestions.length) {
        selectedIndex = -1;
        return;
      }

      selectedIndex = (nextIndex + suggestions.length) % suggestions.length;
      menu.querySelectorAll(".action-search-option").forEach((option) => {
        const optionIndex = Number(option.dataset.index);
        option.classList.toggle("is-selected", optionIndex === selectedIndex);
      });
    };

    const search = async () => {
      const query = input.value.trim();
      if (!query) {
        closeMenu(menu);
        return;
      }

      const token = ++requestToken;
      try {
        const response = await fetch(`${searchUrl}?q=${encodeURIComponent(query)}`);
        if (!response.ok) {
          closeMenu(menu);
          return;
        }

        const items = await response.json();
        if (token !== requestToken) {
          return;
        }

        renderSuggestions(Array.isArray(items) ? items : []);
      } catch (error) {
        closeMenu(menu);
      }
    };

    input.addEventListener("input", function () {
      if (debounceId) {
        window.clearTimeout(debounceId);
      }
      debounceId = window.setTimeout(search, 150);
    });

    input.addEventListener("focus", function () {
      if (input.value.trim()) {
        search();
      }
    });

    input.addEventListener("keydown", function (event) {
      const isOpen = menu.classList.contains("is-open");
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

      if (event.key === "Enter") {
        if (selectedIndex >= 0 && suggestions[selectedIndex]) {
          event.preventDefault();
          selectSuggestion(suggestions[selectedIndex].name);
        }
        return;
      }

      if (event.key === "Escape") {
        closeMenu(menu);
      }
    });
  };

  table
    .querySelectorAll("tr:not(.template) .js-action-item-input")
    .forEach(bindInput);
  return bindInput;
};
