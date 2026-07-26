## ADDED Requirements

### Requirement: User can hide and show the navigation sidebar

The application SHALL allow the user to fully hide the left navigation sidebar and show it again within the same session. When hidden, the sidebar MUST NOT occupy layout space and the workspace MUST use the full content width. Showing the sidebar again MUST restore the same in-session sidebar state that existed immediately before it was hidden (including search query, group collapse state, and current feature selection). The transition MUST be immediate (no animated width change).

#### Scenario: Hide sidebar from View menu

- **WHEN** the sidebar is visible and the user chooses the View menu item that hides the sidebar
- **THEN** the left navigation sidebar is no longer rendered and the workspace expands to the full content width

#### Scenario: Show sidebar from View menu

- **WHEN** the sidebar is hidden and the user chooses the View menu item that shows the sidebar
- **THEN** the left navigation sidebar is rendered again with the same in-session search query, group collapse state, and feature selection as before it was hidden

#### Scenario: Toggle with keyboard shortcut

- **WHEN** the user presses Ctrl+B on Windows/Linux or Cmd+B on macOS
- **THEN** the sidebar visibility toggles exactly as if the corresponding View menu item were chosen

### Requirement: View menu label reflects sidebar visibility

The View menu item that controls sidebar visibility SHALL display a label that depends on the current visibility: when the sidebar is visible the label MUST mean “hide sidebar”; when the sidebar is hidden the label MUST mean “show sidebar”. Labels MUST be provided via i18n for both Simplified Chinese and English, and MUST update after a language switch without incorrectly reverting the visibility-dependent wording.

#### Scenario: Label when sidebar is visible

- **WHEN** the sidebar is visible and the user opens the View menu
- **THEN** the sidebar visibility item is labeled as hide-sidebar in the active language

#### Scenario: Label when sidebar is hidden

- **WHEN** the sidebar is hidden and the user opens the View menu
- **THEN** the sidebar visibility item is labeled as show-sidebar in the active language

#### Scenario: Label follows language switch

- **WHEN** the sidebar is hidden and the user switches UI language
- **THEN** the View menu sidebar item still offers show-sidebar wording in the new language (not hide-sidebar)
