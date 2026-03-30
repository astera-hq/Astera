/**
 * Performance monitoring utilities for the dashboard
 */

interface PerformanceMetrics {
  batchLoadTime: number;
  cacheHitRate: number;
  totalRequests: number;
  cachedRequests: number;
  averageResponseTime: number;
}

class PerformanceMonitor {
  private metrics: PerformanceMetrics = {
    batchLoadTime: 0,
    cacheHitRate: 0,
    totalRequests: 0,
    cachedRequests: 0,
    averageResponseTime: 0,
  };

  private responseTimes: number[] = [];
  private readonly maxResponseTimeSamples = 50;

  /**
   * Record a batch load operation
   */
  recordBatchLoad(duration: number, itemCount: number) {
    this.metrics.batchLoadTime = duration;
    const avgTimePerItem = duration / itemCount;
    this.addResponseTime(avgTimePerItem);
  }

  /**
   * Record a cache hit
   */
  recordCacheHit() {
    this.metrics.cachedRequests++;
    this.metrics.totalRequests++;
    this.updateCacheHitRate();
  }

  /**
   * Record a cache miss
   */
  recordCacheMiss() {
    this.metrics.totalRequests++;
    this.updateCacheHitRate();
  }

  /**
   * Record a response time
   */
  recordResponseTime(duration: number) {
    this.addResponseTime(duration);
  }

  private addResponseTime(duration: number) {
    this.responseTimes.push(duration);
    if (this.responseTimes.length > this.maxResponseTimeSamples) {
      this.responseTimes.shift();
    }
    this.updateAverageResponseTime();
  }

  private updateCacheHitRate() {
    this.metrics.cacheHitRate = 
      this.metrics.totalRequests > 0 
        ? (this.metrics.cachedRequests / this.metrics.totalRequests) * 100 
        : 0;
  }

  private updateAverageResponseTime() {
    if (this.responseTimes.length === 0) {
      this.metrics.averageResponseTime = 0;
      return;
    }
    const sum = this.responseTimes.reduce((acc, time) => acc + time, 0);
    this.metrics.averageResponseTime = sum / this.responseTimes.length;
  }

  /**
   * Get current performance metrics
   */
  getMetrics(): PerformanceMetrics {
    return { ...this.metrics };
  }

  /**
   * Reset all metrics
   */
  reset() {
    this.metrics = {
      batchLoadTime: 0,
      cacheHitRate: 0,
      totalRequests: 0,
      cachedRequests: 0,
      averageResponseTime: 0,
    };
    this.responseTimes = [];
  }

  /**
   * Log performance summary (useful for debugging)
   */
  logSummary() {
    console.log('📊 Performance Summary:', {
      cacheHitRate: `${this.metrics.cacheHitRate.toFixed(1)}%`,
      averageResponseTime: `${this.metrics.averageResponseTime.toFixed(0)}ms`,
      totalRequests: this.metrics.totalRequests,
      batchLoadTime: `${this.metrics.batchLoadTime.toFixed(0)}ms`,
    });
  }
}

// Global performance monitor instance
export const performanceMonitor = new PerformanceMonitor();

/**
 * Higher-order function to measure and record performance of async functions
 */
export function withPerformanceTracking<T extends (...args: any[]) => Promise<any>>(
  fn: T,
  operationName: string
): T {
  return (async (...args: Parameters<T>) => {
    const startTime = performance.now();
    try {
      const result = await fn(...args);
      const duration = performance.now() - startTime;
      performanceMonitor.recordResponseTime(duration);
      
      if (typeof window !== 'undefined' && (window as any).__DEV__) {
        console.log(`⚡ ${operationName}: ${duration.toFixed(0)}ms`);
      }
      
      return result;
    } catch (error) {
      const duration = performance.now() - startTime;
      console.error(`❌ ${operationName} failed after ${duration.toFixed(0)}ms:`, error);
      throw error;
    }
  }) as T;
}

/**
 * Debounce utility to prevent excessive API calls
 */
export function debounce<T extends (...args: any[]) => any>(
  func: T,
  wait: number
): (...args: Parameters<T>) => void {
  let timeout: number;
  return (...args: Parameters<T>) => {
    clearTimeout(timeout);
    timeout = setTimeout(() => func(...args), wait);
  };
}

/**
 * Throttle utility for scroll events
 */
export function throttle<T extends (...args: any[]) => any>(
  func: T,
  limit: number
): (...args: Parameters<T>) => void {
  let inThrottle: boolean;
  return (...args: Parameters<T>) => {
    if (!inThrottle) {
      func(...args);
      inThrottle = true;
      setTimeout(() => inThrottle = false, limit);
    }
  };
}
