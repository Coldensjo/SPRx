import { useState } from 'react';
import { RotateCcw } from 'lucide-react';
import {
	DEFAULT_EXPORT_SETTINGS,
	ExportSettings,
	SheetArrangement
} from './settings';
import type { SheetAlign } from './spr';

interface Props {
	settings: ExportSettings;
	onSave: (settings: ExportSettings) => void;
	onClose: () => void;
}

/** Local draft where numeric fields are strings so inputs can be cleared while typing. */
interface Draft {
	arrangement: SheetArrangement;
	gridBy: 'cols' | 'rows';
	gridCount: string;
	spacing: string;
	align: SheetAlign;
}

function toDraft(s: ExportSettings): Draft {
	return {
		arrangement: s.arrangement,
		gridBy: s.gridBy,
		gridCount: String(s.gridCount),
		spacing: String(s.spacing),
		align: s.align
	};
}

/**
 * Presets dialog for the combined-spritesheet export. Values saved here become
 * the defaults each time the export dialog is opened, persisted across sessions.
 */
export default function ExportSettingsDialog({ settings, onSave, onClose }: Props) {
	const [draft, setDraft] = useState<Draft>(() => toDraft(settings));

	const save = () => {
		onSave({
			arrangement: draft.arrangement,
			gridBy: draft.gridBy,
			gridCount: Math.max(1, Math.min(999, parseInt(draft.gridCount, 10) || 1)),
			spacing: Math.max(0, Math.min(256, parseInt(draft.spacing, 10) || 0)),
			align: draft.align
		});
		onClose();
	};

	return (
		<div className="ss-backdrop" onMouseDown={onClose}>
			<div className="ss-modal" onMouseDown={e => e.stopPropagation()}>
				<div className="ss-modal-title">Export settings</div>
				<div className="ss-modal-desc">Default layout for combined spritesheets.</div>

				<div className="ss-field">
					<span className="ss-field-label">Arrangement</span>
					<select
						value={draft.arrangement}
						onChange={e => setDraft(d => ({ ...d, arrangement: e.target.value as SheetArrangement }))}
					>
						<option value="vertical">Vertical — single column</option>
						<option value="horizontal">Horizontal — single row</option>
						<option value="grid">Grid</option>
					</select>
				</div>

				{draft.arrangement === 'grid' && (
					<div className="ss-field-row">
						<div className="ss-field">
							<span className="ss-field-label">Fixed</span>
							<select
								value={draft.gridBy}
								onChange={e => setDraft(d => ({ ...d, gridBy: e.target.value as 'cols' | 'rows' }))}
							>
								<option value="cols">Columns</option>
								<option value="rows">Rows</option>
							</select>
						</div>
						<div className="ss-field">
							<span className="ss-field-label">Count</span>
							<input
								type="number"
								min={1}
								max={999}
								value={draft.gridCount}
								onChange={e => setDraft(d => ({ ...d, gridCount: e.target.value }))}
								onKeyDown={e => e.key === 'Enter' && save()}
							/>
						</div>
					</div>
				)}

				<div className="ss-field-row">
					<div className="ss-field">
						<span className="ss-field-label">Spacing (px)</span>
						<input
							type="number"
							min={0}
							max={256}
							value={draft.spacing}
							onChange={e => setDraft(d => ({ ...d, spacing: e.target.value }))}
							onKeyDown={e => e.key === 'Enter' && save()}
						/>
					</div>
					<div className="ss-field">
						<span className="ss-field-label">Align</span>
						<select
							value={draft.align}
							onChange={e => setDraft(d => ({ ...d, align: e.target.value as SheetAlign }))}
						>
							<option value="start">Start</option>
							<option value="center">Center</option>
							<option value="end">End</option>
						</select>
					</div>
				</div>

				<div className="ss-modal-buttons">
					<button
						className="ss-btn ss-btn-ghost"
						onClick={() => setDraft(toDraft(DEFAULT_EXPORT_SETTINGS))}
						title="Reset fields to defaults"
					>
						<RotateCcw size={14} />
						Reset
					</button>
					<span className="ss-modal-buttons-spacer" />
					<button className="ss-btn" onClick={onClose}>
						Cancel
					</button>
					<button className="ss-btn ss-btn-primary" onClick={save}>
						Save
					</button>
				</div>
			</div>
		</div>
	);
}
