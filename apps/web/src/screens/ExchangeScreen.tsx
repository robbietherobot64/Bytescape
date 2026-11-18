import { useState, useEffect } from 'react';
import { exchangeApi, ExchangeListingData, ExchangeMessageData } from '../utils/api';
import { userFromToken } from '../utils/api';

const CATEGORIES = [
  { id: '', label: 'All Categories', icon: '📦' },
  { id: 'electronics', label: 'Electronics', icon: '💻' },
  { id: 'furniture', label: 'Furniture', icon: '🪑' },
  { id: 'clothing', label: 'Clothing', icon: '👕' },
  { id: 'books', label: 'Books', icon: '📚' },
  { id: 'vehicles', label: 'Vehicles', icon: '🚗' },
  { id: 'services', label: 'Services', icon: '🔧' },
  { id: 'housing', label: 'Housing', icon: '🏠' },
  { id: 'free', label: 'Free', icon: '🎁' },
  { id: 'sports', label: 'Sports', icon: '⚽' },
  { id: 'music', label: 'Music', icon: '🎵' },
  { id: 'art', label: 'Art', icon: '🎨' },
  { id: 'other', label: 'Other', icon: '📋' },
];

const LISTING_TYPES = [
  { id: 'all', label: 'All' },
  { id: 'offer', label: 'Offers' },
  { id: 'want', label: 'Wanted' },
];

export default function ExchangeScreen() {
  const [listings, setListings] = useState<ExchangeListingData[]>([]);
  const [inbox, setInbox] = useState<ExchangeMessageData[]>([]);
  const [selectedListing, setSelectedListing] = useState<ExchangeListingData | null>(null);
  const [messages, setMessages] = useState<ExchangeMessageData[]>([]);
  const [category, setCategory] = useState('');
  const [listingType, setListingType] = useState('all');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreateModal, setShowCreateModal] = useState(false);

  // Create form state
  const [newTitle, setNewTitle] = useState('');
  const [newDesc, setNewDesc] = useState('');
  const [newType, setNewType] = useState('offer');
  const [newCategory, setNewCategory] = useState('other');
  const [newPrice, setNewPrice] = useState('');
  const [newCurrency, setNewCurrency] = useState('USD');
  const [newLocation, setNewLocation] = useState('');
  const [newTags, setNewTags] = useState('');

  // Message state
  const [messageText, setMessageText] = useState('');

  const currentUser = userFromToken();
  const isOwner = selectedListing && currentUser && selectedListing.user_id === currentUser.sub;

  const loadListings = async () => {
    setLoading(true);
    setError(null);
    try {
      const params = new URLSearchParams();
      if (category) params.set('category', category);
      if (listingType !== 'all') params.set('listing_type', listingType);
      params.set('limit', '50');
      params.set('offset', '0');
      const data = await exchangeApi.listListings(params.toString());
      setListings(data.items);
    } catch (e: any) {
      setError(e.message || 'Failed to load listings');
    } finally {
      setLoading(false);
    }
  };

  const loadInbox = async () => {
    try {
      const data = await exchangeApi.listInbox('limit=50&offset=0');
      setInbox(data);
    } catch {
      setInbox([]);
    }
  };

  useEffect(() => {
    loadListings();
    loadInbox();
  }, [category, listingType]);

  const loadMessages = async (listingId: string) => {
    try {
      const data = await exchangeApi.listMessages(listingId, 'limit=50&offset=0');
      setMessages(data);
    } catch {
      setMessages([]);
    }
  };

  const selectListing = async (listing: ExchangeListingData) => {
    setSelectedListing(listing);
    setMessages([]);
    if (currentUser && listing.user_id === currentUser.sub || listing.status === 'active') {
      try {
        const detail = await exchangeApi.getListing(listing.id);
        setSelectedListing(detail);
      } catch {}
    }
    if (currentUser) {
      await loadMessages(listing.id);
    }
  };

  const handleCreateListing = async () => {
    if (!newTitle.trim() || !newDesc.trim()) return;
    try {
      const tags = newTags.split(',').map(t => t.trim()).filter(Boolean);
      const priceVal = newPrice.trim() ? parseInt(newPrice, 10) : null;
      await exchangeApi.createListing({
        title: newTitle,
        description: newDesc,
        listing_type: newType,
        category: newCategory,
        price_cents: priceVal,
        currency: newCurrency,
        location: newLocation || undefined,
        tags: tags.length > 0 ? tags : undefined,
      });
      setShowCreateModal(false);
      setNewTitle('');
      setNewDesc('');
      setNewType('offer');
      setNewCategory('other');
      setNewPrice('');
      setNewCurrency('USD');
      setNewLocation('');
      setNewTags('');
      await loadListings();
    } catch (e: any) {
      setError(e.message || 'Failed to create listing');
    }
  };

  const handleSendMessage = async () => {
    if (!messageText.trim() || !selectedListing) return;
    try {
      await exchangeApi.sendMessage(selectedListing.id, messageText);
      setMessageText('');
      await loadMessages(selectedListing.id);
    } catch (e: any) {
      setError(e.message || 'Failed to send message');
    }
  };

  const handleUpdateStatus = async (status: string) => {
    if (!selectedListing) return;
    try {
      const updated = await exchangeApi.updateStatus(selectedListing.id, status);
      setSelectedListing(updated);
      await loadListings();
    } catch (e: any) {
      setError(e.message || 'Failed to update status');
    }
  };

  const handleDeleteListing = async () => {
    if (!selectedListing) return;
    try {
      await exchangeApi.deleteListing(selectedListing.id);
      setSelectedListing(null);
      setMessages([]);
      await loadListings();
    } catch (e: any) {
      setError(e.message || 'Failed to delete listing');
    }
  };

  const formatPrice = (listing: ExchangeListingData) => {
    if (listing.price_cents === null) return 'Free / Trade';
    const currency = listing.currency || 'USD';
    const symbols: Record<string, string> = { USD: '$', EUR: '€', GBP: '£', JPY: '¥' };
    const sym = symbols[currency] || currency + ' ';
    return sym + (listing.price_cents / 100).toFixed(2);
  };

  const formatTime = (ts: string) => {
    const d = new Date(ts);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    if (diff < 60000) return 'just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return d.toLocaleDateString();
  };

  const getCategoryIcon = (catId: string) => {
    return CATEGORIES.find(c => c.id === catId)?.icon || '📦';
  };

  return (
    <div className="screen">
      <div className="exchange-layout">
        {/* Left sidebar: categories + type filter */}
        <div className="exchange-sidebar">
          <div className="sidebar-header">
            <span className="sidebar-title">Exchange</span>
          </div>
          <div className="sidebar-section">
            <div className="sidebar-section-title">Type</div>
            <div className="sidebar-categories">
              {LISTING_TYPES.map(lt => (
                <div
                  key={lt.id}
                  className={`category-item ${listingType === lt.id ? 'exchange-cat-active' : ''}`}
                  onClick={() => { setListingType(lt.id); setSelectedListing(null); }}
                >
                  <span className="category-item-label">{lt.label}</span>
                </div>
              ))}
            </div>
          </div>
          <div className="sidebar-section">
            <div className="sidebar-section-title">Categories</div>
            <div className="sidebar-categories">
              {CATEGORIES.map(c => (
                <div
                  key={c.id}
                  className={`category-item ${category === c.id ? 'exchange-cat-active' : ''}`}
                  onClick={() => { setCategory(c.id); setSelectedListing(null); }}
                >
                  <span className="category-icon">{c.icon}</span>
                  <span className="category-item-label">{c.label}</span>
                </div>
              ))}
            </div>
          </div>
          <div className="sidebar-section">
            <button className="btn btn-block exchange-create-btn" onClick={() => setShowCreateModal(true)}>
              + Create Listing
            </button>
          </div>
        </div>

        {/* Center: listing cards */}
        <div className="exchange-center">
          <div className="feed-column-header">
            <h1>
              {CATEGORIES.find(c => c.id === category)?.label || 'All Categories'}
              {listingType !== 'all' ? ` — ${LISTING_TYPES.find(lt => lt.id === listingType)?.label}` : ''}
            </h1>
          </div>

          <div className="exchange-listing-list">
            {loading && <div className="exchange-loading">Loading listings...</div>}
            {error && <div className="exchange-error">{error}</div>}
            {!loading && !error && listings.length === 0 && (
              <div className="exchange-empty">
                <p>No listings found. Create one!</p>
              </div>
            )}
            {!loading && listings.map(listing => (
              <div
                key={listing.id}
                className={`exchange-card ${selectedListing?.id === listing.id ? 'exchange-card-active' : ''}`}
                onClick={() => selectListing(listing)}
              >
                <div className="exchange-card-header">
                  <span className="exchange-card-title">{listing.title}</span>
                  <span className={`exchange-type-badge ${listing.listing_type === 'offer' ? 'exchange-type-offer' : 'exchange-type-want'}`}>
                    {listing.listing_type === 'offer' ? 'OFFER' : 'WANT'}
                  </span>
                </div>
                <div className="exchange-card-meta">
                  <span className="exchange-card-price">{formatPrice(listing)}</span>
                  {listing.location && (
                    <span className="exchange-card-location">📍 {listing.location}</span>
                  )}
                </div>
                <div className="exchange-card-footer">
                  <span className="exchange-category-badge">
                    {getCategoryIcon(listing.category)} {listing.category}
                  </span>
                  {listing.author && (
                    <span className="exchange-card-author">by {listing.author.display_name}</span>
                  )}
                  <span className="exchange-card-time">{formatTime(listing.created_at)}</span>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Right: listing detail + messages */}
        <div className="exchange-detail">
          {selectedListing ? (
            <div className="exchange-detail-content">
              <div className="exchange-detail-header">
                <h2>{selectedListing.title}</h2>
                <span className={`exchange-type-badge exchange-type-lg ${selectedListing.listing_type === 'offer' ? 'exchange-type-offer' : 'exchange-type-want'}`}>
                  {selectedListing.listing_type === 'offer' ? 'OFFER' : 'WANT'}
                </span>
              </div>

              <div className="exchange-detail-meta">
                <div className="exchange-detail-price">{formatPrice(selectedListing)}</div>
                {selectedListing.location && (
                  <div className="exchange-detail-location">📍 {selectedListing.location}</div>
                )}
                <div className="exchange-detail-category">
                  {getCategoryIcon(selectedListing.category)} {selectedListing.category}
                </div>
                <div className="exchange-detail-status">Status: {selectedListing.status}</div>
                <div className="exchange-detail-views">👁 {selectedListing.view_count} views</div>
                {selectedListing.author && (
                  <div className="exchange-detail-author">
                    by {selectedListing.author.display_name} (@{selectedListing.author.handle})
                  </div>
                )}
                <div className="exchange-detail-time">
                  Posted {formatTime(selectedListing.created_at)}
                </div>
              </div>

              {selectedListing.tags.length > 0 && (
                <div className="exchange-tags">
                  {selectedListing.tags.map(tag => (
                    <span key={tag} className="exchange-tag">{tag}</span>
                  ))}
                </div>
              )}

              {selectedListing.image_urls.length > 0 && (
                <div className="exchange-images">
                  {selectedListing.image_urls.map((url, i) => (
                    <img key={i} src={url} alt="" className="exchange-image" />
                  ))}
                </div>
              )}

              <div className="exchange-detail-desc">
                <h3>Description</h3>
                <p>{selectedListing.description}</p>
              </div>

              {/* Owner controls */}
              {isOwner && (
                <div className="exchange-owner-controls">
                  <h3>Manage Listing</h3>
                  <div className="exchange-owner-buttons">
                    {selectedListing.status === 'active' && (
                      <button className="btn" onClick={() => handleUpdateStatus('sold')}>Mark as Sold</button>
                    )}
                    {selectedListing.status === 'sold' && (
                      <button className="btn" onClick={() => handleUpdateStatus('active')}>Reactivate</button>
                    )}
                    {selectedListing.status === 'active' && (
                      <button className="btn" onClick={() => handleUpdateStatus('expired')}>Expire</button>
                    )}
                    <button className="btn btn-danger" onClick={handleDeleteListing}>Delete</button>
                  </div>
                </div>
              )}

              {/* Messages */}
              {currentUser && !isOwner && selectedListing.status === 'active' && (
                <div className="exchange-messages-section">
                  <h3>Contact Seller</h3>
                  <div className="exchange-composer">
                    <textarea
                      className="exchange-message-input"
                      placeholder="Send a message about this listing..."
                      value={messageText}
                      onChange={e => setMessageText(e.target.value)}
                      rows={3}
                    />
                    <button
                      className="btn"
                      onClick={handleSendMessage}
                      disabled={!messageText.trim()}
                    >
                      Send Message
                    </button>
                  </div>
                </div>
              )}

              {messages.length > 0 && (
                <div className="exchange-messages-thread">
                  <h3>Messages ({messages.length})</h3>
                  {messages.map(msg => (
                    <div key={msg.id} className="exchange-message">
                      <div className="exchange-message-header">
                        <span className="exchange-message-author">
                          {msg.sender?.display_name || 'Unknown'}
                        </span>
                        <span className="exchange-message-time">{formatTime(msg.created_at)}</span>
                      </div>
                      <div className="exchange-message-body">{msg.content}</div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ) : (
            <div className="exchange-detail-empty">
              <p>Select a listing to view details</p>
            </div>
          )}
        </div>
      </div>

      {/* Create Listing Modal */}
      {showCreateModal && (
        <div className="modal-overlay" onClick={() => setShowCreateModal(false)}>
          <div className="modal-box" onClick={e => e.stopPropagation()}>
            <h2>Create Listing</h2>
            <div className="form-group">
              <div className="exchange-form-row">
                <label>Type</label>
                <div className="exchange-type-select">
                  <button
                    className={`btn ${newType === 'offer' ? 'btn-active' : ''}`}
                    onClick={() => setNewType('offer')}
                  >
                    Offer
                  </button>
                  <button
                    className={`btn ${newType === 'want' ? 'btn-active' : ''}`}
                    onClick={() => setNewType('want')}
                  >
                    Want
                  </button>
                </div>
              </div>
              <input
                className="input-field"
                placeholder="Title"
                value={newTitle}
                onChange={e => setNewTitle(e.target.value)}
              />
              <textarea
                className="input-field"
                placeholder="Description"
                value={newDesc}
                onChange={e => setNewDesc(e.target.value)}
                rows={4}
                style={{ resize: 'vertical' }}
              />
              <select
                className="input-field"
                value={newCategory}
                onChange={e => setNewCategory(e.target.value)}
              >
                {CATEGORIES.filter(c => c.id).map(c => (
                  <option key={c.id} value={c.id}>{c.label}</option>
                ))}
              </select>
              <div className="exchange-form-row">
                <input
                  className="input-field"
                  type="number"
                  placeholder="Price (cents, e.g. 5000 = $50.00)"
                  value={newPrice}
                  onChange={e => setNewPrice(e.target.value)}
                  style={{ flex: 1 }}
                />
                <select
                  className="input-field"
                  value={newCurrency}
                  onChange={e => setNewCurrency(e.target.value)}
                  style={{ width: 80 }}
                >
                  <option value="USD">$</option>
                  <option value="EUR">€</option>
                  <option value="GBP">£</option>
                  <option value="JPY">¥</option>
                </select>
              </div>
              <input
                className="input-field"
                placeholder="Location (optional)"
                value={newLocation}
                onChange={e => setNewLocation(e.target.value)}
              />
              <input
                className="input-field"
                placeholder="Tags (comma separated)"
                value={newTags}
                onChange={e => setNewTags(e.target.value)}
              />
              <button
                className="btn btn-block"
                onClick={handleCreateListing}
                disabled={!newTitle.trim() || !newDesc.trim()}
              >
                Create Listing
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
