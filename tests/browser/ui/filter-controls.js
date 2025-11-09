let resultsList = null;
let canvasGallery = null;
let activeCanvasFilter = null;

export function initFilterControls({ results, gallery, initialKey }) {
    resultsList = results;
    canvasGallery = gallery;
    activeCanvasFilter = normalizeFilterKey(initialKey);
    applyCanvasFilterState();
}

export function getActiveCanvasFilter() {
    return activeCanvasFilter;
}

export function slugify(label) {
    return label.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

export function normalizeFilterKey(value) {
    return value ? slugify(value) : null;
}

export function toggleCanvasFilter(filterKey) {
    setActiveCanvasFilter(activeCanvasFilter === filterKey ? null : filterKey);
}

export function wireCanvasFilterControls(wrapper, title, filterKey) {
    if (!filterKey || !title) {
        return;
    }
    title.setAttribute("role", "button");
    title.setAttribute("aria-pressed", "false");
    title.tabIndex = 0;
    const toggle = () => toggleCanvasFilter(filterKey);
    title.addEventListener("click", toggle);
    title.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            toggle();
        }
    });
}

export function wireResultFilterControls(element, filterKey) {
    if (!filterKey || !element) {
        return;
    }
    element.dataset.filterKey = filterKey;
    element.setAttribute("role", "button");
    element.setAttribute("aria-pressed", "false");
    element.tabIndex = 0;
    const toggle = () => toggleCanvasFilter(filterKey);
    element.addEventListener("click", toggle);
    element.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            toggle();
        }
    });
}

export function applyCanvasFilterState() {
    if (!canvasGallery || !resultsList) {
        return;
    }
    const wrappers = canvasGallery.querySelectorAll(".canvas-wrapper");
    wrappers.forEach((wrapper) => {
        const matches =
            !activeCanvasFilter || wrapper.dataset.filterKey === activeCanvasFilter;
        wrapper.classList.toggle("hidden", !matches);
        wrapper.classList.toggle("focused-filter", Boolean(activeCanvasFilter && matches));
        const title = wrapper.querySelector("h3");
        if (title) {
            title.setAttribute(
                "aria-pressed",
                activeCanvasFilter && matches ? "true" : "false",
            );
        }
    });
    canvasGallery.classList.toggle("has-filter", Boolean(activeCanvasFilter));

    const resultItems = resultsList.querySelectorAll("li[data-filter-key]");
    resultItems.forEach((item) => {
        const matches = activeCanvasFilter && item.dataset.filterKey === activeCanvasFilter;
        item.classList.toggle("focused-filter", Boolean(matches));
        item.setAttribute("aria-pressed", matches ? "true" : "false");
    });
}

function setActiveCanvasFilter(newKey) {
    if (activeCanvasFilter === newKey) {
        return;
    }
    activeCanvasFilter = newKey;
    updateFilterQueryParam();
    applyCanvasFilterState();
}

function updateFilterQueryParam() {
    const url = new URL(window.location.href);
    if (activeCanvasFilter) {
        url.searchParams.set("test", activeCanvasFilter);
    } else {
        url.searchParams.delete("test");
    }
    window.history.replaceState(null, "", url.toString());
}
