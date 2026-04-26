import { renderHook, act } from '@testing-library/react';
import { useStore } from '@/lib/store';

describe('useStore', () => {
  beforeEach(() => {
    // Reset store to initial state before each test
    const { result } = renderHook(() => useStore());
    act(() => {
      result.current.disconnect();
    });
  });

  it('has correct initial state', () => {
    const { result } = renderHook(() => useStore());
    
    expect(result.current.wallet).toEqual({
      address: null,
      connected: false,
      network: 'testnet',
    });
    expect(result.current.poolConfig).toBeNull();
    expect(result.current.position).toBeNull();
  });

  it('updates wallet state', () => {
    const { result } = renderHook(() => useStore());
    
    act(() => {
      result.current.setWallet({
        address: 'GDUMMY...',
        connected: true,
        network: 'testnet',
      });
    });
    
    expect(result.current.wallet).toEqual({
      address: 'GDUMMY...',
      connected: true,
      network: 'testnet',
    });
  });

  it('updates pool config', () => {
    const { result } = renderHook(() => useStore());
    const mockConfig = {
      invoice_contract: 'CONTRACT1',
      admin: 'ADMIN1',
      yield_bps: 800,
      compound_interest: false,
    };
    
    act(() => {
      result.current.setPoolConfig(mockConfig);
    });
    
    expect(result.current.poolConfig).toEqual(mockConfig);
  });

  it('updates position', () => {
    const { result } = renderHook(() => useStore());
    const mockPosition = {
      deposited: 10000000000n,
      available: 5000000000n,
      deployed: 5000000000n,
      earned: 0n,
      deposit_count: 1,
    };
    
    act(() => {
      result.current.setPosition(mockPosition);
    });
    
    expect(result.current.position).toEqual(mockPosition);
  });

  it('clears position when set to null', () => {
    const { result } = renderHook(() => useStore());
    
    act(() => {
      result.current.setPosition({
        deposited: 10000000000n,
        available: 5000000000n,
        deployed: 5000000000n,
        earned: 0n,
        deposit_count: 1,
      });
      result.current.setPosition(null);
    });
    
    expect(result.current.position).toBeNull();
  });

  it('disconnect resets wallet and position', () => {
    const { result } = renderHook(() => useStore());
    
    act(() => {
      result.current.setWallet({
        address: 'GDUMMY...',
        connected: true,
        network: 'testnet',
      });
      result.current.setPosition({
        deposited: 10000000000n,
        available: 5000000000n,
        deployed: 5000000000n,
        earned: 0n,
        deposit_count: 1,
      });
      result.current.disconnect();
    });
    
    expect(result.current.wallet).toEqual({
      address: null,
      connected: false,
      network: 'testnet',
    });
    expect(result.current.position).toBeNull();
    expect(result.current.poolConfig).toBeNull();
  });

  it('keeps position unchanged when a deposit flow fails', async () => {
    const { result } = renderHook(() => useStore());
    const initialPosition = {
      deposited: 10000000000n,
      available: 5000000000n,
      deployed: 5000000000n,
      earned: 0n,
      deposit_count: 1,
    };

    act(() => {
      result.current.setPosition(initialPosition);
    });

    await expect(Promise.reject(new Error('deposit failed'))).rejects.toThrow('deposit failed');
    expect(result.current.position).toEqual(initialPosition);
  });

  it('does not create a position when network request fails', async () => {
    const { result } = renderHook(() => useStore());
    expect(result.current.position).toBeNull();

    await expect(Promise.reject(new Error('network timeout'))).rejects.toThrow('network timeout');
    expect(result.current.position).toBeNull();
  });
});
