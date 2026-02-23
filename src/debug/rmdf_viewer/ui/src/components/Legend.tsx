interface LegendProps {
  style?: React.CSSProperties;
}

const LEGEND_ITEMS = [
  { color: '#3388ff', label: 'Default (no flags)' },
  { color: '#ff9800', label: 'Residential in proximity' },
  { color: '#f44336', label: 'No-go area' },
];

const DIRECTION_ITEMS = [
  { symbol: '——', label: 'Both ways' },
  { symbol: '- -', label: 'One way (dashed)' },
  { symbol: '▶', label: 'Direction arrow' },
];

export function Legend({ style }: LegendProps) {
  return (
    <div 
      style={{ 
        padding: '0.75rem',
        background: 'rgba(255, 255, 255, 0.95)',
        borderRadius: '4px',
        boxShadow: '0 2px 6px rgba(0,0,0,0.2)',
        fontSize: '0.85rem',
        ...style,
      }}
    >
      <h4 style={{ margin: '0 0 0.5rem 0', fontSize: '0.9rem' }}>Color Legend</h4>
      
      <div style={{ marginBottom: '0.75rem' }}>
        {LEGEND_ITEMS.map(item => (
          <div 
            key={item.label}
            style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.25rem' }}
          >
            <span 
              style={{ 
                width: '16px', 
                height: '16px', 
                background: item.color, 
                borderRadius: '50%',
                border: '1px solid #333',
              }} 
            />
            <span>{item.label}</span>
          </div>
        ))}
      </div>
      
      <h4 style={{ margin: '0 0 0.5rem 0', fontSize: '0.9rem' }}>Line Styles</h4>
      <div>
        {DIRECTION_ITEMS.map(item => (
          <div 
            key={item.label}
            style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.25rem' }}
          >
            <span 
              style={{ 
                width: '16px', 
                textAlign: 'center',
                fontFamily: 'monospace',
                fontSize: '0.8rem',
              }} 
            >
              {item.symbol}
            </span>
            <span>{item.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
