const themedVars = (light: boolean): Record<string, string> => ({
  "--background": light ? "#f3fbf7" : "#0b1512",
  "--divider": light ? "rgba(38, 124, 88, 0.22)" : "rgba(134, 236, 183, 0.24)",
  "--icon-primary": light ? "#10211b" : "#eef8f2",
  "--icon-secondary": light ? "#3f5f52" : "#a8bdb3",
  "--tip-color": light ? "#536a60" : "#9fb5ab",
  "--btn-primary": light ? "rgba(31, 135, 91, 0.12)" : "rgba(112, 234, 176, 0.14)",
  "--btn-primary-hover": light ? "rgba(31, 135, 91, 0.2)" : "rgba(112, 234, 176, 0.24)",
  "--btn-secondary": light ? "rgba(16, 33, 27, 0.07)" : "rgba(238, 248, 242, 0.1)",
  "--btn-secondary-hover": light ? "rgba(16, 33, 27, 0.12)" : "rgba(238, 248, 242, 0.16)",
});

const connectorShadowCss = `
  :host {
    z-index: 90;
    color: var(--icon-primary);
  }

  .background {
    background: rgba(3, 8, 7, 0.72) !important;
    backdrop-filter: blur(18px) saturate(1.08);
  }

  .main {
    min-width: min(23rem, calc(100vw - 32px));
    background: var(--background) !important;
    border: 1px solid var(--divider);
    box-shadow:
      0 28px 90px rgba(0, 0, 0, 0.42),
      inset 0 1px 0 rgba(255, 255, 255, 0.06);
  }

  ccc-selecting-scene,
  ccc-connected-scene,
  ccc-dialog {
    color: var(--icon-primary);
  }
`;

export const applyCCCConnectorTheme = (connector: HTMLElement) => {
  connector.classList.add("cellscript-ccc-connector");
  connector.style.setProperty("z-index", "90");

  const light = document.documentElement.dataset.theme === "light";
  for (const [name, value] of Object.entries(themedVars(light))) {
    connector.style.setProperty(name, value);
  }

  const installShadowStyle = () => {
    const root = (connector as HTMLElement & { shadowRoot?: ShadowRoot | null }).shadowRoot;
    if (!root || root.querySelector("[data-cellscript-ccc-theme]")) return;
    const style = document.createElement("style");
    style.dataset.cellscriptCccTheme = "";
    style.textContent = connectorShadowCss;
    root.appendChild(style);
  };

  installShadowStyle();
  connector.addEventListener("willUpdate", installShadowStyle);
  requestAnimationFrame(installShadowStyle);
  window.setTimeout(installShadowStyle, 0);
  window.setTimeout(installShadowStyle, 160);
};
