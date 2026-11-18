# Real-Time Notifications System

## Overview

The real-time notifications system automatically polls contract events from Soroban RPC and displays them to users via toast notifications and the notification bell. Users see funding, repayment, and dispute events as they happen without needing to manually refresh the page.

## Architecture

### Components

1. **RealTimeNotificationProvider** (`components/RealTimeNotificationProvider.tsx`)
   - Client component that activates the polling service on mount
   - Detects user role based on wallet and pool config
   - Configures polling with appropriate settings

2. **Polling Service** (`lib/sse-events.ts`)
   - `SseEventsService` singleton that manages event polling
   - Polls Soroban RPC every 15 seconds (configurable)
   - Filters events by user role (Admin/SME/Investor)
   - Shows toast notifications for relevant events
   - Dispatches notifications through `notificationService`

3. **useSseEvents Hook** (`hooks/useSseEvents.ts`)
   - React hook that manages service lifecycle
   - Starts polling on mount, stops on unmount
   - Handles visibility changes (pauses when tab is hidden)

4. **Toast Notifications** (`components/Toast.tsx`)
   - Custom toast system that displays event notifications
   - Supports info, warning, error, and success variants
   - Auto-dismisses after configurable duration

5. **Notification Bell** (`components/NotificationBell.tsx`)
   - Displays all received notifications
   - Subscribes to `notificationService`
   - Shows priority badges (LOW/MEDIUM/HIGH/CRITICAL)

### Data Flow

```
Soroban RPC
    ↓
Polling Service (15s interval)
    ↓
Event Processing
    ├→ Toast Notification (pushToast)
    ├→ NotificationService dispatch
    └→ Store update (Zustand)
    ↓
UI Updates
├→ Toast display
└→ NotificationBell update
```

## Supported Events

The polling service tracks and notifies on these contract events:

### Invoice Events

- `funded` - Invoice has been funded by pool
- `repaid` - Invoice principal + interest repaid
- `default` - Invoice marked as defaulted
- `created` - New invoice created

### Pool Events

- `deposit` - Investor deposits to pool
- `withdraw` - Investor withdraws from pool

## Configuration

### Polling Interval

Default: 15 seconds (configurable)

```typescript
// In RealTimeNotificationProvider.tsx
useSseEvents({
  role: userRole,
  intervalMs: 15_000, // Change this to adjust polling frequency
  enabled: true,
});
```

Constraints:

- Minimum: 5 seconds
- Maximum: No limit (but avoid excessive polling to reduce RPC load)

### User Role Detection

The provider automatically detects the user's role:

```typescript
- Admin: User's wallet address matches pool admin address
- Other: Default to Admin for full event coverage
```

To refine role detection, modify the logic in `RealTimeNotificationProvider.tsx`:

```typescript
const userRole: UserRole = (() => {
  if (!wallet.connected || !wallet.address) {
    return 'Admin';
  }

  if (poolConfig?.admin === wallet.address) {
    return 'Admin';
  }

  // Add more sophisticated role detection here
  // e.g., check invoice ownership for SME role
  // e.g., check investor positions for Investor role

  return 'Admin';
})();
```

### Event Filtering

Each role receives different events via `ROLE_EVENT_FILTERS` in `lib/sse-events.ts`:

```typescript
SME: ['funded', 'repaid', 'default', 'created'];
Investor: ['funded', 'repaid', 'deposit', 'withdraw', 'default'];
Admin: ['funded', 'repaid', 'default', 'created', 'deposit', 'withdraw'];
```

## Testing

### Manual Testing

1. **Start the app**:

   ```bash
   npm run dev
   ```

2. **Open the browser console** to see polling logs:

   ```
   [RealTimeNotifications] Polling activated for role: Admin
   [SSE Events] Started polling every 15000ms (role: Admin)
   [SSE Events] Received N new event(s).
   ```

3. **Trigger contract events**:
   - Create a new invoice
   - Fund an invoice
   - Repay an invoice
   - Mark invoice as defaulted

4. **Verify notifications appear**:
   - Toast notifications should appear in the top-right corner
   - Notification bell should update with new messages
   - No page refresh required

### Debugging

Enable verbose logging by checking the browser console:

```typescript
// Look for these log patterns:
[SSE Events] Polling started
[SSE Events] Poll cycle failed: [error details]
[SSE Events] Received X new event(s)
[Astera Alert] [PRIORITY] TYPE: message
```

### Disabling Polling

To temporarily disable polling (e.g., for testing manual refreshes):

```typescript
// In RealTimeNotificationProvider.tsx
useSseEvents({
  role: userRole,
  intervalMs: 15_000,
  enabled: false, // Set to false to disable
});
```

## Troubleshooting

### Notifications not appearing

1. **Check if polling is active**:
   - Open browser console
   - Look for `[SSE Events] Started polling` message
   - Verify `RealTimeNotificationProvider` is rendered in the component tree

2. **Check browser console for errors**:
   - Look for network errors in Network tab
   - Check if RPC endpoint is accessible
   - Verify environment variables are set correctly

3. **Verify event filtering**:
   - Check user role detection in console logs
   - Verify events match the role's filter list

### High CPU/Memory usage

If polling is consuming excessive resources:

1. **Increase polling interval** (see Configuration section)
2. **Check for memory leaks**:
   - Verify polling stops when component unmounts
   - Check browser DevTools Memory tab for retained objects

3. **Reduce event history**:
   - Modify `MAX_EVENTS_HISTORY` in `lib/sse-events.ts` (default: 100)

### Events not firing

1. **Verify contract events are being emitted**:
   - Use Stellar Expert explorer to check recent transactions
   - Verify events exist on-chain before expecting them in the UI

2. **Check RPC connection**:
   - Verify `NEXT_PUBLIC_SOROBAN_RPC_URL` env variable
   - Test RPC endpoint directly: `curl https://soroban-testnet.stellar.org/`

3. **Check ledger lookback range**:
   - Service looks back 50 ledgers by default
   - If events are older, they won't be fetched
   - Modify `startLedger` calculation in `SseEventsService.fetchNewEvents()`

## Performance Considerations

- **Polling Interval**: 15 seconds is a good balance between responsiveness and RPC load
- **Event History**: Limited to 100 events to prevent memory issues
- **Visibility Aware**: Polling pauses when tab is hidden, saving resources
- **Deduplication**: Events are tracked by ID to prevent duplicate notifications

## Future Enhancements

1. **WebSocket SSE**: Replace polling with true Server-Sent Events for lower latency
2. **Smart Role Detection**: Automatically detect SME/Investor based on contract interactions
3. **User Preferences**: Allow users to customize notification settings
4. **Persistence**: Store notification history in IndexedDB
5. **Sound Alerts**: Optional audio notification for critical events
6. **Mobile Support**: Optimize notifications for mobile devices

## Related Files

- `frontend/lib/sse-events.ts` - Main polling service
- `frontend/hooks/useSseEvents.ts` - React hook
- `frontend/components/RealTimeNotificationProvider.tsx` - Provider component
- `frontend/lib/notifications.ts` - Notification service
- `frontend/components/NotificationBell.tsx` - Notification UI
- `frontend/components/Toast.tsx` - Toast system
- `frontend/app/api/events/route.ts` - Events API endpoint
