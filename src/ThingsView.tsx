import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { Copy, FileImage, Grid3X3, Loader2, Pause, Play, Search, X, ZoomIn, ZoomOut } from 'lucide-react';
import {
	exportThing,
	getThing,
	getThings,
	OpenDat,
	OpenFile,
	ThingCategory,
	ThingDetail,
	ThingSummary,
	thingsRowUrl,
	thingUrl
} from './spr';
import type { Toast } from './App';

const ZOOM_LEVELS = [48, 64, 96, 128];
const GRID_PAD = 8;
const ANIM_INTERVAL_MS = 220;
const DIRECTIONS = ['North', 'East', 'South', 'West'];

interface Props {
	spr: OpenFile;
	dat: OpenDat;
	category: ThingCategory;
	selectedId: number | null;
	onSelect: (id: number) => void;
	showToast: (kind: Toast['kind'], msg: string) => void;
}

interface MenuState {
	x: number;
	y: number;
	id: number;
}

const CATEGORY_LABEL: Record<ThingCategory, string> = {
	item: 'Item',
	outfit: 'Outfit',
	effect: 'Effect',
	missile: 'Missile'
};

interface RowProps {
	spr: OpenFile;
	dat: OpenDat;
	category: ThingCategory;
	things: ThingSummary[];
	top: number;
	zoom: number;
	cellW: number;
	cellH: number;
	transparent: boolean;
	selectedId: number | null;
	onSelect: (id: number) => void;
	onContextMenu: (e: React.MouseEvent, id: number) => void;
}

// One atlas image per grid row: each thing occupies a zoom×zoom square.
const ThingRow = memo(function ThingRow({
	spr,
	dat,
	category,
	things,
	top,
	zoom,
	cellW,
	cellH,
	transparent,
	selectedId,
	onSelect,
	onContextMenu
}: RowProps) {
	const url = thingsRowUrl(
		spr,
		dat,
		category,
		things.map(t => t.id),
		zoom,
		transparent
	);
	return (
		<div className="ss-grid-row" style={{ top, paddingLeft: GRID_PAD }}>
			{things.map((t, i) => (
				<div
					key={t.id}
					className={`ss-cell ${selectedId === t.id ? 'ss-cell-selected' : ''}`}
					style={{ width: cellW, height: cellH }}
					title={t.name ? `${t.id} — ${t.name}` : String(t.id)}
					onMouseDown={e => e.button === 0 && onSelect(t.id)}
					onContextMenu={e => onContextMenu(e, t.id)}
				>
					<div
						className="ss-cell-sprite"
						style={{
							width: zoom,
							height: zoom,
							backgroundImage: `url("${url}")`,
							backgroundSize: `${things.length * zoom}px ${zoom}px`,
							backgroundPosition: `-${i * zoom}px 0`,
							backgroundRepeat: 'no-repeat'
						}}
					/>
					<div className="ss-cell-id">{t.id}</div>
				</div>
			))}
		</div>
	);
});

function parseIdSearch(query: string): ((id: number) => boolean) | null {
	const trimmed = query.trim();
	if (!/^[\d\s,;-]+$/.test(trimmed)) return null;
	const ranges: [number, number][] = [];
	for (const token of trimmed.split(/[,;\s]+/)) {
		if (!token) continue;
		const range = token.match(/^(\d+)\s*-\s*(\d+)$/);
		if (range) {
			let a = parseInt(range[1], 10);
			let b = parseInt(range[2], 10);
			if (a > b) [a, b] = [b, a];
			ranges.push([a, b]);
		} else if (/^\d+$/.test(token)) {
			const id = parseInt(token, 10);
			ranges.push([id, id]);
		}
	}
	if (ranges.length === 0) return null;
	return id => ranges.some(([a, b]) => id >= a && id <= b);
}

export default function ThingsView({ spr, dat, category, selectedId, onSelect, showToast }: Props) {
	const [things, setThings] = useState<ThingSummary[] | null>(null);
	const [loadError, setLoadError] = useState<string | null>(null);
	const [search, setSearch] = useState('');
	const [transparent, setTransparent] = useState(false);
	const [zoomIdx, setZoomIdx] = useState(1);
	const [detail, setDetail] = useState<ThingDetail | null>(null);
	const [menu, setMenu] = useState<MenuState | null>(null);
	const [playing, setPlaying] = useState(true);
	const [frame, setFrame] = useState(0);
	const [dir, setDir] = useState(2); // south

	const scrollRef = useRef<HTMLDivElement>(null);
	const [scrollTop, setScrollTop] = useState(0);
	const [viewport, setViewport] = useState({ w: 0, h: 0 });

	const zoom = ZOOM_LEVELS[zoomIdx];
	const cellW = zoom + 16;
	const cellH = zoom + 16 + 16;

	useEffect(() => {
		let cancelled = false;
		getThings(dat.path, category)
			.then(list => {
				if (!cancelled) setThings(list);
			})
			.catch(e => {
				if (!cancelled) setLoadError(String(e));
			});
		return () => {
			cancelled = true;
		};
	}, [dat.path, category]);

	// Details pane is always open: default to the first thing of the category.
	useEffect(() => {
		if (things && things.length > 0 && selectedId === null) {
			onSelect(things[0].id);
		}
	}, [things, selectedId, onSelect]);

	useEffect(() => {
		const el = scrollRef.current;
		if (!el) return;
		const ro = new ResizeObserver(() => setViewport({ w: el.clientWidth, h: el.clientHeight }));
		ro.observe(el);
		setViewport({ w: el.clientWidth, h: el.clientHeight });
		return () => ro.disconnect();
	}, [things]);

	// Fetch full detail for the selected thing
	useEffect(() => {
		if (selectedId === null) {
			setDetail(null);
			return;
		}
		let cancelled = false;
		getThing(dat.path, category, selectedId)
			.then(d => {
				if (!cancelled) {
					setDetail(d);
					setFrame(0);
					setDir(d.isOutfit && d.patternX >= 3 ? 2 : 0);
				}
			})
			.catch(() => {
				if (!cancelled) setDetail(null);
			});
		return () => {
			cancelled = true;
		};
	}, [selectedId, dat.path, category]);

	// Animation loop for the preview
	useEffect(() => {
		if (!detail || detail.frames <= 1 || !playing) return;
		const t = setInterval(() => setFrame(f => (f + 1) % detail.frames), ANIM_INTERVAL_MS);
		return () => clearInterval(t);
	}, [detail, playing]);

	const shown = useMemo(() => {
		if (!things) return [];
		const q = search.trim();
		if (!q) return things;
		const byId = parseIdSearch(q);
		if (byId) return things.filter(t => byId(t.id));
		const needle = q.toLowerCase();
		return things.filter(t => t.name?.toLowerCase().includes(needle));
	}, [things, search]);

	const cols = Math.max(1, Math.floor((viewport.w - GRID_PAD * 2) / cellW));
	const rows = Math.ceil(shown.length / cols);
	const totalHeight = rows * cellH + GRID_PAD * 2;
	const firstRow = Math.max(0, Math.floor((scrollTop - GRID_PAD) / cellH) - 2);
	const lastRow = Math.min(rows - 1, Math.ceil((scrollTop + viewport.h) / cellH) + 2);

	const visible: { row: number; things: ThingSummary[] }[] = [];
	for (let r = firstRow; r <= lastRow; r++) {
		const slice = shown.slice(r * cols, (r + 1) * cols);
		if (slice.length > 0) visible.push({ row: r, things: slice });
	}

	const handleContextMenu = useCallback(
		(e: React.MouseEvent, id: number) => {
			e.preventDefault();
			onSelect(id);
			setMenu({ x: e.clientX, y: e.clientY, id });
		},
		[onSelect]
	);

	useEffect(() => {
		const onClick = () => setMenu(null);
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape') setMenu(null);
		};
		window.addEventListener('mousedown', onClick);
		window.addEventListener('keydown', onKey);
		return () => {
			window.removeEventListener('mousedown', onClick);
			window.removeEventListener('keydown', onKey);
		};
	}, []);

	const doExport = useCallback(
		async (id: number, mode: 'image' | 'sheet') => {
			const suffix = mode === 'sheet' ? 'sheet' : 'image';
			const out = await saveDialog({
				defaultPath: `${category}_${id}_${suffix}.png`,
				filters: [{ name: 'PNG image', extensions: ['png'] }]
			});
			if (!out) return;
			try {
				await exportThing(spr.path, dat.path, category, id, mode, transparent, out);
				showToast('ok', `Exported ${category} ${id} (${mode === 'sheet' ? 'spritesheet' : 'image'})`);
			} catch (e) {
				showToast('error', String(e));
			}
		},
		[spr.path, dat.path, category, transparent, showToast]
	);

	if (loadError) {
		return (
			<div className="ss-loading">
				<span>
					Failed to load {category}s: {loadError}
				</span>
			</div>
		);
	}

	if (!things) {
		return (
			<div className="ss-loading">
				<Loader2 size={16} className="ss-spin" />
				<span>Loading {category}s…</span>
			</div>
		);
	}

	const menuThing = menu ? things.find(t => t.id === menu.id) ?? null : null;
	const isAnimated = detail !== null && detail.frames > 1;
	const showDirs = detail !== null && detail.isOutfit && detail.patternX >= 2;

	return (
		<>
			<div className="ss-toolbar">
				<div className="ss-search">
					<Search size={14} />
					<input
						placeholder={`Search client ID (e.g. 2400 or 100-250)${category === 'item' ? ' or name' : ''}`}
						value={search}
						onChange={e => setSearch(e.target.value)}
						spellCheck={false}
					/>
					{search && (
						<button className="ss-search-clear" onClick={() => setSearch('')} aria-label="Clear search">
							<X size={13} />
						</button>
					)}
				</div>

				<label className="ss-toggle">
					<input type="checkbox" checked={transparent} onChange={e => setTransparent(e.target.checked)} />
					Transparency
				</label>

				<div className="ss-zoom">
					<button
						className="ss-zoom-btn"
						onClick={() => setZoomIdx(i => Math.max(0, i - 1))}
						disabled={zoomIdx === 0}
						aria-label="Zoom out"
					>
						<ZoomOut size={14} />
					</button>
					<span className="ss-zoom-label">{zoom}px</span>
					<button
						className="ss-zoom-btn"
						onClick={() => setZoomIdx(i => Math.min(ZOOM_LEVELS.length - 1, i + 1))}
						disabled={zoomIdx === ZOOM_LEVELS.length - 1}
						aria-label="Zoom in"
					>
						<ZoomIn size={14} />
					</button>
				</div>
			</div>

			<div className="ss-things-body">
				<div className="ss-grid-wrap" ref={scrollRef} onScroll={e => setScrollTop(e.currentTarget.scrollTop)}>
					<div className="ss-grid-inner" style={{ height: totalHeight }}>
						{shown.length === 0 && <div className="ss-grid-empty">No {category}s match the current filter.</div>}
						{visible.map(({ row, things: rowThings }) => (
							<ThingRow
								key={`${row}-${rowThings[0].id}-${rowThings.length}`}
								spr={spr}
								dat={dat}
								category={category}
								things={rowThings}
								top={GRID_PAD + row * cellH}
								zoom={zoom}
								cellW={cellW}
								cellH={cellH}
								transparent={transparent}
								selectedId={selectedId}
								onSelect={onSelect}
								onContextMenu={handleContextMenu}
							/>
						))}
					</div>
				</div>

				<div className="ss-details">
					<div className="ss-details-header">
						<span>{detail ? `${CATEGORY_LABEL[category]} ${detail.id}` : CATEGORY_LABEL[category]}</span>
						{isAnimated && (
							<button
								className="ss-search-clear"
								onClick={() => setPlaying(p => !p)}
								aria-label={playing ? 'Pause animation' : 'Play animation'}
							>
								{playing ? <Pause size={14} /> : <Play size={14} />}
							</button>
						)}
					</div>
					{detail ? (
						<div className="ss-details-scroll">
							<div className="ss-details-preview">
								{/* All frames stay mounted so the browser caches them; only the
								    current one is visible, giving flicker-free animation. */}
								{Array.from({ length: detail.frames }).map((_, f) => (
									<img
										key={f}
										src={thingUrl(spr, dat, category, detail.id, transparent, f, showDirs ? dir : undefined)}
										style={{
											imageRendering: 'pixelated',
											display: f === frame ? 'block' : 'none'
										}}
										width={detail.width * 32 * (detail.width > 1 || detail.height > 1 ? 1 : 2)}
										draggable={false}
										alt=""
									/>
								))}
							</div>
							{isAnimated && (
								<div className="ss-details-anim">
									frame {frame + 1}/{detail.frames}
								</div>
							)}
							{showDirs && (
								<div className="ss-details-dirs">
									{DIRECTIONS.slice(0, Math.min(4, detail.patternX)).map((label, i) => (
										<button
											key={label}
											className={`ss-dir-btn ${dir === i ? 'ss-dir-btn-active' : ''}`}
											onClick={() => setDir(i)}
										>
											{label[0]}
										</button>
									))}
								</div>
							)}
							{detail.name && <div className="ss-details-name">“{detail.name}”</div>}
							<dl className="ss-details-stats">
								<dt>Size</dt>
								<dd>
									{detail.width}×{detail.height} tiles
								</dd>
								<dt>Layers</dt>
								<dd>{detail.layers}</dd>
								<dt>Patterns</dt>
								<dd>
									{detail.patternX}×{detail.patternY}×{detail.patternZ}
								</dd>
								<dt>Frames</dt>
								<dd>{detail.frames}</dd>
								<dt>Sprites</dt>
								<dd>{detail.spriteIndex.length}</dd>
							</dl>
							{detail.props.length > 0 && (
								<>
									<div className="ss-details-section">Properties</div>
									<dl className="ss-details-stats">
										{detail.props.map((p, i) => (
											<div key={i} className="ss-details-prop">
												<dt>{p.name}</dt>
												<dd>{p.value ?? '✓'}</dd>
											</div>
										))}
									</dl>
								</>
							)}
							<div className="ss-details-section">Sprite IDs</div>
							<div className="ss-details-sprites mono">
								{detail.spriteIndex.slice(0, 120).join(', ')}
								{detail.spriteIndex.length > 120 && ` … +${detail.spriteIndex.length - 120} more`}
							</div>
							<div className="ss-details-actions">
								<button className="ss-btn" onClick={() => void doExport(detail.id, 'image')}>
									<FileImage size={14} />
									Export PNG…
								</button>
								<button className="ss-btn ss-btn-primary" onClick={() => void doExport(detail.id, 'sheet')}>
									<Grid3X3 size={14} />
									Export spritesheet…
								</button>
							</div>
						</div>
					) : (
						<div className="ss-details-empty">Select a {category} to inspect it</div>
					)}
				</div>
			</div>

			<div className="ss-statusbar">
				<span className="ss-status-path">{dat.path}</span>
				<span className="mono">sig 0x{dat.signature.toString(16).toUpperCase().padStart(8, '0')}</span>
				<span>detected v{(dat.version / 100).toFixed(2)}</span>
				<span className="ss-status-spacer" />
				<span>{shown.length.toLocaleString()} shown</span>
				{selectedId !== null && <span className="mono">#{selectedId}</span>}
			</div>

			{menu && menuThing && (
				<div
					className="ss-context-menu"
					style={{
						left: Math.min(menu.x, window.innerWidth - 280),
						top: Math.min(menu.y, window.innerHeight - 160)
					}}
					onMouseDown={e => e.stopPropagation()}
				>
					<button
						className="ss-menu-item"
						onClick={() => {
							setMenu(null);
							void navigator.clipboard.writeText(String(menu.id));
							showToast('ok', `Copied client ID ${menu.id}`);
						}}
					>
						<Copy size={14} />
						Copy client ID {menu.id}
					</button>
					<div className="ss-menu-sep" />
					<button className="ss-menu-item" onClick={() => (setMenu(null), void doExport(menu.id, 'image'))}>
						<FileImage size={14} />
						Export image as PNG…
					</button>
					<button className="ss-menu-item" onClick={() => (setMenu(null), void doExport(menu.id, 'sheet'))}>
						<Grid3X3 size={14} />
						Export spritesheet ({menuThing.patternX * menuThing.layers}×
						{menuThing.frames * menuThing.patternY * menuThing.patternZ} cells)…
					</button>
				</div>
			)}
		</>
	);
}
