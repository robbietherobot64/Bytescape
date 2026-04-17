import { ProfileModule } from '../../types';
import { BsPencilFill, BsPaletteFill, BsTrash, BsChevronUp, BsChevronDown, BsArrowsFullscreen, BsArrowLeft, BsArrowRight } from 'react-icons/bs';

interface ModuleCardProps {
    mod: ProfileModule;
    groupModules: ProfileModule[];
    editMode: boolean;
    previewMode: boolean;
    editingModuleTitle: string | null;
    editingModuleContent: string | null;
    editingModuleStyle: string | null;
    onSetEditMode: (v: boolean) => void;
    onSetEditingModuleTitle: (id: string | null) => void;
    onSetEditingModuleContent: (id: string | null) => void;
    onSetEditingModuleStyle: (id: string | null) => void;
    onUpdateModuleTitle: (id: string, title: string) => void;
    onUpdateModuleStyle: (id: string, style: Partial<ProfileModule>) => void;
    onMoveModule: (id: string, dir: 'up' | 'down') => void;
    onMoveModuleColumn: (id: string) => void;
    onRemoveModule: (id: string) => void;
    onToggleModuleWidth: (id: string) => void;
    renderContent: (mod: ProfileModule) => React.ReactNode;
}

export function ModuleCard({
    mod, groupModules, editMode, previewMode,
    editingModuleTitle, editingModuleContent, editingModuleStyle,
    onSetEditMode, onSetEditingModuleTitle, onSetEditingModuleContent, onSetEditingModuleStyle,
    onUpdateModuleTitle, onUpdateModuleStyle,
    onMoveModule, onMoveModuleColumn, onRemoveModule, onToggleModuleWidth,
    renderContent,
}: ModuleCardProps) {
    const groupIdx = groupModules.indexOf(mod);
    const modStyle: React.CSSProperties = {};
    if (mod.background) { modStyle.background = mod.background; modStyle.backgroundSize = 'cover'; modStyle.backgroundPosition = 'center'; }
    if (mod.font_color) modStyle.color = mod.font_color;
    if (mod.font_size) modStyle.fontSize = mod.font_size;
    if (mod.font_family) modStyle.fontFamily = mod.font_family;

    return (
        <div
            key={mod.id}
            className={`profile-module profile-module--${mod.width} ${editMode ? 'profile-module--editing' : ''}`}
            style={modStyle}
        >
            <div className="profile-module-header">
                {editMode && (
                    <div className="profile-module-reorder">
                        <button className="profile-module-arrow-btn" onClick={() => onMoveModule(mod.id, 'up')} disabled={groupIdx === 0} title="Move up"><BsChevronUp size={10} /></button>
                        <button className="profile-module-arrow-btn" onClick={() => onMoveModule(mod.id, 'down')} disabled={groupIdx === groupModules.length - 1} title="Move down"><BsChevronDown size={10} /></button>
                        {mod.width === 'half' && (
                            <button className="profile-module-arrow-btn" onClick={() => onMoveModuleColumn(mod.id)} title={`Move ${(mod.column ?? 0) === 0 ? 'right' : 'left'}`}>
                                {(mod.column ?? 0) === 0 ? <BsArrowRight size={10} /> : <BsArrowLeft size={10} />}
                            </button>
                        )}
                    </div>
                )}
                {editMode && editingModuleTitle === mod.id ? (
                    <input
                        className="profile-module-title-input"
                        value={mod.title}
                        onChange={(e) => onUpdateModuleTitle(mod.id, e.target.value)}
                        onBlur={() => onSetEditingModuleTitle(null)}
                        onKeyDown={(e) => { if (e.key === 'Enter') onSetEditingModuleTitle(null); }}
                        autoFocus
                    />
                ) : (
                    <span className="profile-module-header-title" onDoubleClick={() => editMode && onSetEditingModuleTitle(mod.id)}>{mod.title}</span>
                )}
                {editMode && (
                    <div className="profile-module-edit-controls">
                        <button className="profile-module-edit-btn" onClick={() => onSetEditingModuleStyle(editingModuleStyle === mod.id ? null : mod.id)} title="Style"><BsPaletteFill size={9} /></button>
                        <button className="profile-module-edit-btn" onClick={() => onSetEditingModuleContent(editingModuleContent === mod.id ? null : mod.id)} title="Edit content"><BsPencilFill size={9} /></button>
                        <button className="profile-module-edit-btn profile-module-edit-btn--danger" onClick={() => onRemoveModule(mod.id)} title="Remove"><BsTrash size={9} /></button>
                    </div>
                )}
                {!editMode && !previewMode && (
                    <button
                        className="profile-module-quick-edit"
                        onClick={() => { onSetEditMode(true); onSetEditingModuleContent(mod.id); }}
                        title="Edit this module"
                    >
                        <BsPencilFill size={9} />
                    </button>
                )}
            </div>

            {editMode && editingModuleStyle === mod.id && (
                <div className="profile-module-style-editor">
                    <div className="profile-module-style-row">
                        <label>Font:</label>
                        <select value={mod.font_family || ''} onChange={(e) => onUpdateModuleStyle(mod.id, { font_family: e.target.value || undefined })}>
                            <option value="">Default</option>
                            <option value="serif">Serif</option>
                            <option value="'Courier New', monospace">Monospace</option>
                            <option value="'Comic Sans MS', cursive">Comic Sans</option>
                            <option value="'Georgia', serif">Georgia</option>
                            <option value="'Impact', sans-serif">Impact</option>
                        </select>
                    </div>
                    <div className="profile-module-style-row">
                        <label>Size:</label>
                        <select value={mod.font_size || ''} onChange={(e) => onUpdateModuleStyle(mod.id, { font_size: e.target.value || undefined })}>
                            <option value="">Default</option>
                            <option value="11px">Small</option>
                            <option value="13px">Medium</option>
                            <option value="15px">Large</option>
                            <option value="18px">XL</option>
                        </select>
                    </div>
                    <div className="profile-module-style-row">
                        <label>Text:</label>
                        <input type="color" value={mod.font_color || '#eeeeee'} onChange={(e) => onUpdateModuleStyle(mod.id, { font_color: e.target.value })} />
                        <button className="profile-customize-reset-btn" onClick={() => onUpdateModuleStyle(mod.id, { font_color: undefined })}>×</button>
                    </div>
                    <div className="profile-module-style-row">
                        <label>BG:</label>
                        <input type="color" value={mod.background || '#1a1a2e'} onChange={(e) => onUpdateModuleStyle(mod.id, { background: e.target.value })} />
                        <button className="profile-customize-reset-btn" onClick={() => onUpdateModuleStyle(mod.id, { background: undefined })}>×</button>
                    </div>
                </div>
            )}

            <div className="profile-module-body">
                {renderContent(mod)}
            </div>
            {editMode && (
                <button
                    className="profile-module-resize-toggle"
                    onClick={() => onToggleModuleWidth(mod.id)}
                    title={mod.width === 'full' ? 'Shrink to half' : 'Expand to full'}
                >
                    <BsArrowsFullscreen />
                </button>
            )}
        </div>
    );
}
