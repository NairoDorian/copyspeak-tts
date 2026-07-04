type SaveBarOnSave = () => Promise<void>;
type SaveBarOnCancel = () => void;

export const saveBar = $state({
  visible: false,
  isSaving: false,
  onSave: null as SaveBarOnSave | null,
  onCancel: null as SaveBarOnCancel | null,
  saveLabel: "",
  cancelLabel: ""
});

export function showSaveBar(
  onSave: SaveBarOnSave,
  onCancel: SaveBarOnCancel,
  saveLabel: string,
  cancelLabel: string
) {
  saveBar.onSave = onSave;
  saveBar.onCancel = onCancel;
  saveBar.saveLabel = saveLabel;
  saveBar.cancelLabel = cancelLabel;
  saveBar.visible = true;
}

export function hideSaveBar() {
  saveBar.visible = false;
  saveBar.onSave = null;
  saveBar.onCancel = null;
}
