import { ClassValue } from "clsx";
import { cn } from "../util";

export const tableClass = (...inputs: ClassValue[]) =>
  cn(
    "w-full text-sm text-left rtl:text-right text-gray-500 dark:text-gray-400",
    inputs,
  );

export const theadClass = (...inputs: ClassValue[]) =>
  cn(
    "text-xs text-gray-700 uppercase bg-gray-50 dark:bg-gray-700 dark:text-gray-400",
    inputs,
  );

export const thClass = (...inputs: ClassValue[]) => cn("px-1 py-1", inputs);

export const trClass = (...inputs: ClassValue[]) =>
  cn("bg-white border-b dark:bg-gray-800 dark:border-gray-700", inputs);

export const tdClass = (...inputs: ClassValue[]) => cn("px-1 py-1", inputs);
