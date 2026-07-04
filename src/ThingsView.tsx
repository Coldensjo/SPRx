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
const MISSILE_DIRECTION_LABELS: Record<string, string> = {
	'0,0': 'NW',
	'1,0': 'N',
	'2,0': 'NE',
	'0,1': 'W',
	'2,1': 'E',
	'0,2': 'SW',
	'1,2': 'S',
	'2,2': 'SE'
};

function thingAnimates(frames: number, animateEnabled: boolean): boolean {
	return frames > 1 && animateEnabled;
}

function defaultAnimateEnabled(category: ThingCategory): boolean {
	return category !== 'item';
}

interface Props {
	spr: OpenFile;
	dat: OpenDat;
	category: ThingCategory;
	selectedId: number | null;
	onSelect: (id: number) => void;
	transparent: boolean;
	onTransparentChange: (transparent: boolean) => void;
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
	cells: GridCell[];
	showAllMissileDirections: boolean;
	top: number;
	zoom: number;
	cellW: number;
	cellH: number;
	transparent: boolean;
	gridFrame: number;
	animateEnabled: boolean;
	selectedId: number | null;
	onSelect: (id: number) => void;
	onContextMenu: (e: React.MouseEvent, id: number) => void;
}

interface GridCell {
	key: string;
	thing: ThingSummary;
	label: string;
	title: string;
}

function defaultGridCell(thing: ThingSummary): GridCell {
	return {
		key: String(thing.id),
		thing,
		label: String(thing.id),
		title: thing.name ? `${thing.id} — ${thing.name}` : String(thing.id)
	};
}

function missileDirectionSlots(thing: ThingSummary): Array<{ key: string; dir: number; diry: number; label: string } | null> {
	const slots: Array<{ key: string; dir: number; diry: number; label: string } | null> = [];
	for (let py = 0; py < thing.patternY; py++) {
		for (let px = 0; px < thing.patternX; px++) {
			if (thing.patternX === 3 && thing.patternY === 3 && px === 1 && py === 1) {
				slots.push(null);
				continue;
			}
			const label = MISSILE_DIRECTION_LABELS[`${px},${py}`] ?? `${px},${py}`;
			slots.push({ key: `${px}-${py}`, dir: px, diry: py, label });
		}
	}
	return slots;
}

const AnimatedThingCell = memo(function AnimatedThingCell({
	spr,
	dat,
	category,
	thing,
	zoom,
	transparent,
	frame,
	dir,
	diry
}: {
	spr: OpenFile;
	dat: OpenDat;
	category: ThingCategory;
	thing: ThingSummary;
	zoom: number;
	transparent: boolean;
	frame: number;
	dir?: number;
	diry?: number;
}) {
	const shown = frame % thing.frames;
	return (
		<div className="ss-cell-sprite ss-cell-sprite-anim" style={{ width: zoom, height: zoom }}>
			{Array.from({ length: thing.frames }, (_, f) => (
				<img
					key={f}
					src={thingUrl(spr, dat, category, thing.id, transparent, f, dir, diry)}
					style={{ display: f === shown ? 'block' : 'none' }}
					width={zoom}
					height={zoom}
					draggable={false}
					alt=""
				/>
			))}
		</div>
	);
});

const StaticThingCell = memo(function StaticThingCell({
	spr,
	dat,
	category,
	thing,
	zoom,
	transparent,
	dir,
	diry
}: {
	spr: OpenFile;
	dat: OpenDat;
	category: ThingCategory;
	thing: ThingSummary;
	zoom: number;
	transparent: boolean;
	dir?: number;
	diry?: number;
}) {
	return (
		<div className="ss-cell-sprite" style={{ width: zoom, height: zoom }}>
			<img
				src={thingUrl(spr, dat, category, thing.id, transparent, 0, dir, diry)}
				width={zoom}
				height={zoom}
				draggable={false}
				alt=""
			/>
		</div>
	);
});

const MissileDirectionGrid = memo(function MissileDirectionGrid({
	spr,
	dat,
	thing,
	zoom,
	transparent,
	frame,
	animateEnabled
}: {
	spr: OpenFile;
	dat: OpenDat;
	thing: ThingSummary;
	zoom: number;
	transparent: boolean;
	frame: number;
	animateEnabled: boolean;
}) {
	const shownFrame = thingAnimates(thing.frames, animateEnabled) ? frame % thing.frames : 0;
	const slots = missileDirectionSlots(thing);
	return (
		<div
			className="ss-missile-dir-grid"
			style={{
				gridTemplateColumns: `repeat(${thing.patternX}, ${zoom}px)`,
				gridTemplateRows: `repeat(${thing.patternY}, ${zoom}px)`
			}}
		>
			{slots.map((slot, i) =>
				slot ? (
					<div key={slot.key} className="ss-missile-dir-cell" title={`${thing.id} ${slot.label}`}>
						<img
							src={thingUrl(spr, dat, 'missile', thing.id, transparent, shownFrame, slot.dir, slot.diry)}
							width={zoom}
							height={zoom}
							draggable={false}
							alt=""
						/>
					</div>
				) : (
					<div key={`empty-${i}`} className="ss-missile-dir-cell ss-missile-dir-cell-empty" />
				)
			)}
		</div>
	);
});

// One atlas image per grid row: each thing occupies a zoom×zoom square.
const ThingRow = memo(function ThingRow({
	spr,
	dat,
	category,
	cells,
	showAllMissileDirections,
	top,
	zoom,
	cellW,
	cellH,
	transparent,
	gridFrame,
	animateEnabled,
	selectedId,
	onSelect,
	onContextMenu
}: RowProps) {
	const things = cells.map(cell => cell.thing);
	const hasMissileDirectionGrids = category === 'missile' && showAllMissileDirections;
	const rowAnimates = animateEnabled && things.some(t => t.frames > 1);
	const atlasUrl =
		rowAnimates || hasMissileDirectionGrids
			? null
			: thingsRowUrl(
					spr,
					dat,
					category,
					things.map(t => t.id),
					zoom,
					transparent
				);
	return (
		<div className="ss-grid-row" style={{ top, paddingLeft: GRID_PAD }}>
			{cells.map((cell, i) => (
				<div
					key={cell.key}
					className={`ss-cell ${selectedId === cell.thing.id ? 'ss-cell-selected' : ''}`}
					style={{ width: cellW, height: cellH }}
					title={cell.title}
					onMouseDown={e => e.button === 0 && onSelect(cell.thing.id)}
					onContextMenu={e => onContextMenu(e, cell.thing.id)}
				>
					{hasMissileDirectionGrids ? (
						<MissileDirectionGrid
							spr={spr}
							dat={dat}
							thing={cell.thing}
							zoom={zoom}
							transparent={transparent}
							frame={gridFrame}
							animateEnabled={animateEnabled}
						/>
					) : rowAnimates && thingAnimates(cell.thing.frames, animateEnabled) ? (
						<AnimatedThingCell
							spr={spr}
							dat={dat}
							category={category}
							thing={cell.thing}
							zoom={zoom}
							transparent={transparent}
							frame={gridFrame}
						/>
					) : atlasUrl ? (
						<div
							className="ss-cell-sprite"
							style={{
								width: zoom,
								height: zoom,
								backgroundImage: `url("${atlasUrl}")`,
								backgroundSize: `${things.length * zoom}px ${zoom}px`,
								backgroundPosition: `-${i * zoom}px 0`,
								backgroundRepeat: 'no-repeat'
							}}
						/>
					) : (
						<StaticThingCell
							spr={spr}
							dat={dat}
							category={category}
							thing={cell.thing}
							zoom={zoom}
							transparent={transparent}
						/>
					)}
					<div className="ss-cell-id">{cell.label}</div>
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

export default function ThingsView({
	spr,
	dat,
	category,
	selectedId,
	onSelect,
	transparent,
	onTransparentChange,
	showToast
}: Props) {
	const [things, setThings] = useState<ThingSummary[] | null>(null);
	const [loadError, setLoadError] = useState<string | null>(null);
	const [search, setSearch] = useState('');
	const [zoomIdx, setZoomIdx] = useState(1);
	const [detail, setDetail] = useState<ThingDetail | null>(null);
	const [menu, setMenu] = useState<MenuState | null>(null);
	const [playing, setPlaying] = useState(true);
	const [frame, setFrame] = useState(0);
	const [dir, setDir] = useState(2); // south
	const [animateEnabled, setAnimateEnabled] = useState(() => defaultAnimateEnabled(category));
	const [showAllMissileDirections, setShowAllMissileDirections] = useState(false);
	const [gridFrame, setGridFrame] = useState(0);

	useEffect(() => {
		setAnimateEnabled(defaultAnimateEnabled(category));
		setShowAllMissileDirections(false);
		setPlaying(true);
	}, [category]);

	const scrollRef = useRef<HTMLDivElement>(null);
	const [scrollTop, setScrollTop] = useState(0);
	const [viewport, setViewport] = useState({ w: 0, h: 0 });

	const zoom = ZOOM_LEVELS[zoomIdx];
	const showMissileDirectionGrids = category === 'missile' && showAllMissileDirections;
	const cellW = (showMissileDirectionGrids ? zoom * 3 : zoom) + 16;
	const cellH = (showMissileDirectionGrids ? zoom * 3 : zoom) + 16 + 16;

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

	// Animation loop for the details preview
	useEffect(() => {
		if (!detail || !thingAnimates(detail.frames, animateEnabled) || !playing) return;
		const t = setInterval(() => setFrame(f => (f + 1) % detail.frames), ANIM_INTERVAL_MS);
		return () => clearInterval(t);
	}, [detail, playing, animateEnabled]);

	useEffect(() => {
		if (!detail || thingAnimates(detail.frames, animateEnabled)) return;
		setFrame(0);
	}, [detail, animateEnabled]);

	// A search doesn't filter the grid — it jumps to the first matching thing.
	const matchFn = useMemo(() => {
		const q = search.trim();
		if (!q) return null;
		const byId = parseIdSearch(q);
		if (byId) return (t: ThingSummary) => byId(t.id);
		const needle = q.toLowerCase();
		return (t: ThingSummary) => !!t.name?.toLowerCase().includes(needle);
	}, [search]);

	const shown = things ?? [];

	const gridCells = useMemo(
		() => shown.map(defaultGridCell),
		[shown]
	);

	const cols = Math.max(1, Math.floor((viewport.w - GRID_PAD * 2) / cellW));
	const rows = Math.ceil(gridCells.length / cols);
	const totalHeight = rows * cellH + GRID_PAD * 2;
	const firstRow = Math.max(0, Math.floor((scrollTop - GRID_PAD) / cellH) - 2);
	const lastRow = Math.min(rows - 1, Math.ceil((scrollTop + viewport.h) / cellH) + 2);

	const visible: { row: number; cells: GridCell[] }[] = [];
	for (let r = firstRow; r <= lastRow; r++) {
		const slice = gridCells.slice(r * cols, (r + 1) * cols);
		if (slice.length > 0) visible.push({ row: r, cells: slice });
	}

	// When the search matches something, select it and scroll it into view.
	useEffect(() => {
		if (!matchFn || !things || cols < 1) return;
		const idx = things.findIndex(matchFn);
		if (idx < 0) return;
		onSelect(things[idx].id);
		const el = scrollRef.current;
		if (!el) return;
		const row = Math.floor(idx / cols);
		const cellTop = GRID_PAD + row * cellH;
		// Only scroll if the target row isn't already comfortably in view.
		if (cellTop < el.scrollTop || cellTop + cellH > el.scrollTop + el.clientHeight) {
			el.scrollTop = Math.max(0, cellTop - (el.clientHeight - cellH) / 2);
		}
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [matchFn, things, cols, cellH]);

	const gridAnimates = useMemo(
		() => animateEnabled && gridCells.some(cell => cell.thing.frames > 1),
		[gridCells, animateEnabled]
	);

	useEffect(() => {
		if (!gridAnimates) {
			setGridFrame(0);
			return;
		}
		const t = setInterval(() => setGridFrame(f => f + 1), ANIM_INTERVAL_MS);
		return () => clearInterval(t);
	}, [gridAnimates]);

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
	const hasFrames = detail !== null && detail.frames > 1;
	const isPlaying = hasFrames && thingAnimates(detail!.frames, animateEnabled) && playing;
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
					<input type="checkbox" checked={transparent} onChange={e => onTransparentChange(e.target.checked)} />
					Transparency
				</label>

				<label className="ss-toggle">
					<input type="checkbox" checked={animateEnabled} onChange={e => setAnimateEnabled(e.target.checked)} />
					Animate
				</label>

				{category === 'missile' && (
					<label className="ss-toggle">
						<input
							type="checkbox"
							checked={showAllMissileDirections}
							onChange={e => setShowAllMissileDirections(e.target.checked)}
						/>
						All directions
					</label>
				)}

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
						{shown.length === 0 && <div className="ss-grid-empty">No {category}s to display.</div>}
						{visible.map(({ row, cells: rowCells }) => (
							<ThingRow
								key={`${row}-${rowCells[0].key}-${rowCells.length}`}
								spr={spr}
								dat={dat}
								category={category}
								cells={rowCells}
								showAllMissileDirections={showAllMissileDirections}
								top={GRID_PAD + row * cellH}
								zoom={zoom}
								cellW={cellW}
								cellH={cellH}
								transparent={transparent}
								gridFrame={gridFrame}
								animateEnabled={animateEnabled}
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
						{hasFrames && (
							<button
								className="ss-search-clear"
								onClick={() => setPlaying(p => !p)}
								disabled={!animateEnabled}
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
							{isPlaying && (
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
				<span>{gridCells.length.toLocaleString()} shown</span>
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
