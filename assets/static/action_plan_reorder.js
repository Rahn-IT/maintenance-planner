window.addEventListener("load", function () {
  const table = document.getElementById("items");
  if (!table || !table.tBodies || !table.tBodies[0]) {
    return;
  }

  const body = table.tBodies[0];
  let draggingRow = null;

  const clearDropMarkers = () => {
    body.querySelectorAll("tr").forEach((row) => {
      row.classList.remove("drag-over-top");
      row.classList.remove("drag-over-bottom");
    });
  };

  const bindRow = (row) => {
    if (!row || row.classList.contains("template")) {
      return;
    }
    if (row.dataset.reorderBound === "true") {
      return;
    }

    row.dataset.reorderBound = "true";
    row.draggable = true;

    row.addEventListener("dragstart", function (event) {
      // Only start drag when user grabbed the explicit handle button.
      if (row.dataset.dragReady !== "true") {
        event.preventDefault();
        return;
      }
      draggingRow = row;
      row.classList.add("is-dragging");
      if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", row.rowIndex.toString());
      }
    });

    row.addEventListener("dragend", function () {
      row.classList.remove("is-dragging");
      row.dataset.dragReady = "false";
      draggingRow = null;
      clearDropMarkers();
    });

    const handle = row.querySelector(".drag-handle");
    if (handle) {
      handle.addEventListener("mousedown", function () {
        row.dataset.dragReady = "true";
      });
      handle.addEventListener("mouseup", function () {
        row.dataset.dragReady = "false";
      });
      handle.addEventListener("mouseleave", function () {
        row.dataset.dragReady = "false";
      });
    }
  };

  body.addEventListener("dragover", function (event) {
    if (!draggingRow) {
      return;
    }

    const targetRow = event.target.closest("tr");
    if (!targetRow || targetRow === draggingRow || targetRow.classList.contains("template")) {
      return;
    }

    event.preventDefault();
    clearDropMarkers();

    const rect = targetRow.getBoundingClientRect();
    const before = event.clientY < rect.top + rect.height / 2;
    targetRow.classList.add(before ? "drag-over-top" : "drag-over-bottom");
  });

  body.addEventListener("drop", function (event) {
    if (!draggingRow) {
      return;
    }

    const targetRow = event.target.closest("tr");
    if (!targetRow || targetRow === draggingRow || targetRow.classList.contains("template")) {
      return;
    }

    event.preventDefault();

    const rect = targetRow.getBoundingClientRect();
    const before = event.clientY < rect.top + rect.height / 2;
    if (before) {
      body.insertBefore(draggingRow, targetRow);
    } else {
      body.insertBefore(draggingRow, targetRow.nextSibling);
    }
    clearDropMarkers();
  });

  body.querySelectorAll("tr").forEach(bindRow);

  const observer = new MutationObserver((mutations) => {
    mutations.forEach((mutation) => {
      mutation.addedNodes.forEach((node) => {
        if (node instanceof HTMLTableRowElement) {
          bindRow(node);
        }
      });
    });
  });
  observer.observe(body, { childList: true });
});
