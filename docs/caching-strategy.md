# Caching Strategy Documentation

This document explains the SWR (stale-while-revalidate) caching strategy used in the Astera frontend to optimize RPC calls and improve user experience.

## Overview

The Astera frontend uses SWR for data fetching with a comprehensive caching strategy designed to:

- Minimize redundant RPC calls to reduce costs and latency
- Provide timely updates for time-sensitive data
- Maintain data consistency across the application
- Handle cache invalidation appropriately after state changes

## Cache Configuration

### Base Configuration

```typescript
const SWR_CONFIG = {
  refreshInterval: 30000, // 30 seconds default
  revalidateOnFocus: true,
  revalidateOnReconnect: true,
  dedupingInterval: 5000, // 5 seconds
};
```

### Cache Durations by Data Type

| Data Type | TTL | Reason |
|------------|-----|--------|
| **poolConfig** | 5 minutes (300s) | Changes infrequently (admin updates) |
| **invoiceCount** | 15 seconds | Changes with new invoices |
| **invoice** | 10 seconds | Status changes frequently |
| **position** | 15 seconds | Changes with deposits/commits |
| **tokens** | 1 minute (60s) | Whitelist changes rarely |
| **tokenTotals** | 20 seconds | Changes with deposits/deployments |
| **fundedInvoice** | 10 seconds | Status changes frequently |

### Cache Keys Structure

SWR uses a consistent key structure for cache identification:

```typescript
// Single items
'pool-config'                    // Pool configuration
'accepted-tokens'                 // Whitelisted tokens
'invoice-count'                  // Total invoice count
['invoice', id]                  // Specific invoice
['invoice-metadata', id]           // Invoice metadata
['position', investor, token]       // Investor position
['token-totals', token]           // Pool token totals
['funded-invoice', invoiceId]       // Funded invoice details

// Mutations
'deposit'                        // Deposit operations
'withdraw'                       // Withdrawal operations
'commit'                         // Invoice commitments
'create-invoice'                 // Invoice creation
'mark-defaulted'                // Default marking
'init-cofunding'                 // Co-funding initialization
'set-yield'                      // Yield rate changes
```

## Cache Invalidation Strategy

### Automatic Invalidation

After any state-changing transaction, relevant cache keys are invalidated to ensure data consistency:

```typescript
// After deposit/withdrawal - invalidate position and totals
mutate('pool-position');        // Revalidate investor position
mutate('token-totals');        // Revalidate pool totals

// After invoice operations - invalidate invoice-related caches
getInvoiceCacheKeys(invoiceId).forEach(key => mutate(key));

// After configuration changes - invalidate pool-wide caches
mutate('pool-config');
mutate('accepted-tokens');
```

### Manual Invalidation

Components can trigger cache refreshes using SWR's `mutate` function:

```typescript
import { mutate } from 'swr';

// Refresh all invoice-related data
getInvoiceCacheKeys().forEach(key => mutate(key));

// Refresh specific investor position
mutate(['position', investorAddress, tokenAddress]);
```

## Deduplication

The `dedupingInterval` (5 seconds) prevents multiple components from triggering identical requests within a short time window:

```typescript
// Multiple components requesting same data within 5s = 1 actual RPC call
usePoolConfig(); // Component A
usePoolConfig(); // Component B (within 5s) = uses cached response
```

## Error Handling

### Retry Strategy

SWR automatically retries failed requests with exponential backoff. Custom error handling provides user-friendly messages:

```typescript
class ContractError extends Error {
  constructor(message: string, public code?: string) {
    super(message);
    this.name = 'ContractError';
  }
}

async function fetcher<T>(fn: () => Promise<T>): Promise<T> {
  try {
    return await fn();
  } catch (error) {
    if (error instanceof Error) {
      throw new ContractError(error.message);
    }
    throw new ContractError('Unknown error occurred');
  }
}
```

### Error Boundaries

Each SWR hook can be wrapped with error boundaries to provide graceful degradation:

```typescript
const { data, error, isLoading } = usePoolConfig();

if (error) {
  return <ErrorDisplay error={error} />;
}

if (isLoading) {
  return <LoadingSpinner />;
}
```

## Performance Optimizations

### Selective Revalidation

Only revalidate data that's actually affected by state changes:

```typescript
// GOOD: Targeted invalidation
mutate(['position', investor, token]);
mutate(['token-totals', token]);

// AVOID: Broad invalidation
mutate(); // Revalidates everything - expensive!
```

### Optimistic Updates

For better UX, implement optimistic updates where appropriate:

```typescript
const triggerDeposit = useSWRMutation('deposit', depositMutation, {
  onSuccess: () => {
    // Invalidate caches to show updated data
    mutate(['position', investor, token]);
    mutate(['token-totals', token]);
  },
  onError: (error) => {
    // Rollback optimistic update if needed
    toast.error(error.message);
  }
});
```

## Monitoring and Debugging

### Cache Inspection

Use browser DevTools to monitor cache behavior:

1. Open Network tab
2. Filter by XHR/Fetch requests
3. Look for duplicate requests within deduplication window
4. Verify cache TTLs are respected

### Performance Metrics

Key metrics to monitor:

- **RPC Call Frequency**: Should decrease significantly with proper caching
- **Cache Hit Rate**: Most requests should hit cache within TTL
- **Time to Data**: Cached data should load instantly
- **Revalidation Frequency**: Should respect configured intervals

## Best Practices

### DO ✅

- Use specific cache keys for granular invalidation
- Set appropriate TTLs based on data volatility
- Implement targeted cache invalidation after mutations
- Handle errors gracefully with retry logic
- Monitor cache performance in production

### DON'T ❌

- Don't invalidate entire cache for small changes
- Don't set very long TTLs for frequently changing data
- Don't ignore cache invalidation after state changes
- Don't make RPC calls without cache considerations
- Don't disable revalidation unless absolutely necessary

## Adding New Cached Endpoints

When adding new contract calls to the cache system:

1. **Define appropriate TTL** based on data change frequency
2. **Create specific cache key** following the established pattern
3. **Add to cache helpers** for invalidation groups
4. **Update documentation** with new endpoint details
5. **Test cache behavior** with DevTools

Example:
```typescript
// 1. Add to STALE_TIMES
const STALE_TIMES = {
  // ... existing times
  newEndpoint: 45000, // 45 seconds
};

// 2. Create cache hook
export function useNewEndpoint(param: string | null) {
  return useSWR<NewDataType, ContractError>(
    param ? ['new-endpoint', param] : null,
    () => fetcher(() => getNewEndpoint(param!)),
    {
      ...SWR_CONFIG,
      refreshInterval: STALE_TIMES.newEndpoint,
    }
  );
}

// 3. Add to invalidation helpers
export function getNewEndpointCacheKeys(param?: string) {
  const keys: (string | (string | number)[])[] = ['new-endpoint'];
  if (param !== undefined) {
    keys.push(['new-endpoint', param]);
  }
  return keys;
}
```

## Troubleshooting

### Common Issues

**Problem**: Stale data showing
**Solution**: Check TTL settings and manual invalidation triggers

**Problem**: Too many RPC calls
**Solution**: Verify deduplication is working, check cache key consistency

**Problem**: Data not updating after mutations
**Solution**: Ensure proper cache invalidation in mutation success handlers

**Problem**: Memory usage high
**Solution**: Review TTL settings, consider garbage collection strategies

This caching strategy provides a robust foundation for efficient data fetching while maintaining data consistency across the Astera application.
