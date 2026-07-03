import { FolderOpen, History, Loader2 } from 'lucide-react';

interface Props {
	recent: string[];
	error: string | null;
	opening: boolean;
	dropActive: boolean;
	onPick: () => void;
	onOpenRecent: (path: string) => void;
}

export default function Landing({ recent, error, opening, dropActive, onPick, onOpenRecent }: Props) {
	return (
		<div className="ss-landing">
			<div className={`ss-landing-card ${dropActive ? 'ss-drop-active' : ''}`}>
				<img src="/icon.png" alt="" className="ss-landing-icon" width={44} height={44} />
				<div className="ss-landing-title">SPRx</div>
				<div className="ss-landing-subtitle">
					Browse items, outfits, effects and missiles from any Tibia client and export them as PNG.
					<br />
					Open a Tibia.dat or Tibia.spr — the matching file is found automatically.
				</div>

				{error && <div className="ss-landing-error">{error}</div>}

				<button className="ss-btn ss-btn-primary" onClick={onPick} disabled={opening}>
					{opening ? <Loader2 size={15} className="ss-spin" /> : <FolderOpen size={15} />}
					{opening ? 'Opening…' : 'Open client files'}
				</button>

				{recent.length > 0 && (
					<div className="ss-recent">
						<div className="ss-recent-header">Recent files</div>
						{recent.map(path => (
							<button key={path} className="ss-recent-row" onClick={() => onOpenRecent(path)} disabled={opening}>
								<History size={13} />
								<span className="ss-recent-path">{path}</span>
							</button>
						))}
					</div>
				)}
			</div>
		</div>
	);
}
