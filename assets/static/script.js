window.onload = function () {
  const completeExecutionLink = document.querySelector(".execution-complete-link");

  const updateCompleteExecutionLinkState = () => {
    if (!completeExecutionLink) {
      return;
    }

    const checkboxes = Array.from(document.querySelectorAll(".execution-item-toggle"));
    const allChecked = checkboxes.length > 0 && checkboxes.every((checkbox) => checkbox.checked);
    completeExecutionLink.classList.toggle("is-disabled", !allChecked);
    completeExecutionLink.setAttribute("aria-disabled", allChecked ? "false" : "true");
  };

  const bindRemoveButton = (button) => {
    if (!button) {
      return;
    }

    button.addEventListener("click", function () {
      const row = this.closest("tr");
      if (row) {
        row.remove();
      }
    });
  };

  const initializeDynamicRows = () => {
    document.querySelectorAll(".add-row").forEach((button) => {
      button.addEventListener("click", function () {
        const tableId = this.getAttribute("data-table");
        const table = document.getElementById(tableId);
        if (!table) {
          return;
        }

        const tableBody = table.tBodies[0] || table;
        const templateRow = tableBody.querySelector(".template");
        if (!templateRow) {
          return;
        }

        const newRow = templateRow.cloneNode(true);
        newRow.classList.remove("template");
        newRow.querySelectorAll("input").forEach((input) => {
          input.removeAttribute("form");
        });
        tableBody.appendChild(newRow);
        bindRemoveButton(newRow.querySelector(".remove"));
      });
    });

    document.querySelectorAll(".remove").forEach(bindRemoveButton);
  };

  const initializeExecutionItemToggles = () => {
    document.querySelectorAll(".execution-item-toggle").forEach((checkbox) => {
      checkbox.addEventListener("change", async function () {
        const previousChecked = !this.checked;
        const url = this.getAttribute("data-url");
        this.disabled = true;

        try {
          const response = await fetch(url, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
            },
            body: JSON.stringify({ finished: this.checked }),
          });

          if (!response.ok) {
            this.checked = previousChecked;
            alert("Could not update item status.");
            return;
          }

          const payload = await response.json();
          const row = this.closest("tr");
          const finishedAt = row ? row.querySelector(".finished-at") : null;
          if (finishedAt) {
            finishedAt.textContent = payload.finished_display
              ? `Finished: ${payload.finished_display}`
              : "";
          }
        } catch (error) {
          this.checked = previousChecked;
          alert("Could not update item status.");
        } finally {
          this.disabled = false;
          updateCompleteExecutionLinkState();
        }
      });
    });
  };

  const initializeCompletionLink = () => {
    if (!completeExecutionLink) {
      return;
    }

    completeExecutionLink.addEventListener("click", function (event) {
      if (this.getAttribute("aria-disabled") === "true") {
        event.preventDefault();
      }
    });

    updateCompleteExecutionLinkState();
  };

  const initializeClickableCards = () => {
    document.querySelectorAll(".plan-card-clickable").forEach((card) => {
      const navigate = () => {
        const href = card.getAttribute("data-href");
        if (href) {
          window.location.href = href;
        }
      };

      card.addEventListener("click", function (event) {
        if (event.target.closest("a, button, input, form, label, textarea, select")) {
          return;
        }
        navigate();
      });

      card.addEventListener("keydown", function (event) {
        if (event.key !== "Enter" && event.key !== " ") {
          return;
        }
        event.preventDefault();
        navigate();
      });
    });
  };

  initializeDynamicRows();
  initializeExecutionItemToggles();
  initializeCompletionLink();
  initializeClickableCards();
};
