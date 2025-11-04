import { binarySearchBy } from './binarySearch';
import { describe, it, expect } from '@jest/globals';

describe('binarySearchBy', () => {
    it('should find the correct index in a sorted array of numbers', () => {
        const arr = [1, 3, 5, 7, 9];
        const index = binarySearchBy(arr, 6, (x) => x);
        expect(index).toBe(3); // 6 should be inserted at index 3
    });

    it('should return 0 for an empty array', () => {
        const arr: number[] = [];
        const index = binarySearchBy(arr, 10, (x) => x);
        expect(index).toBe(0);
    });

    it('should find the correct index in a sorted array of objects', () => {
        const arr = [{ value: 1 }, { value: 3 }, { value: 5 }, { value: 7 }];
        const index = binarySearchBy(arr, 4, (x) => x.value);
        expect(index).toBe(2); // 4 should be inserted at index 2
    });

    it('should handle duplicates correctly', () => {
        const arr = [1, 2, 2, 2, 3];
        const index = binarySearchBy(arr, 2, (x) => x);
        expect(index).toBe(1); // First occurrence of 2 is at index 1
    });
});
