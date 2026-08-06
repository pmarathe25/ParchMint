# Launcher and project creation

- **PRJ-001:** ParchMint must start at the launcher in v1. It must not automatically reopen previous projects.
- **PRJ-002:** The launcher must show recent projects and actions to create or open a project.
- **PRJ-003:** New Project must collect project title, destination, and optional author. The project format stores `en-US` as its fixed spellcheck language value for forward compatibility; New Project does not ask the user to choose a language.
- **PRJ-004:** The suggested directory name may derive from the project title, but later title changes must not rename or move the directory.
- **PRJ-005:** New projects must contain one Manuscript document named `Untitled Document`, open it immediately, and begin Research empty.
- **PRJ-006:** One project opens in one application window; multiple project windows may be open.
- **PRJ-007:** A project may have only one writable ParchMint project session at a time. A second open attempt should focus the existing window when possible or show a safe locked-project message.
- **PRJ-008:** ParchMint must refuse to create a project inside another Git working tree in v1.
- **PRJ-009:** Project directory paths appear only in project-management contexts, not throughout the writing UI.
- **PRJ-010:** Each recent-project entry must present the project name, project directory path, and last-opened date and time. Activating the project name opens the project.
