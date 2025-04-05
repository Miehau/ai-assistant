// src/lib/services/fileService.ts
import { invoke } from '@tauri-apps/api/tauri';
import type { FileMetadata } from '$lib/types';

/**
 * Service for handling file operations using the Rust backend
 */
export class FileService {
  /**
   * Upload a file to the Rust backend
   * @param fileData Base64 encoded file data
   * @param fileName Name of the file
   * @param mimeType MIME type of the file
   * @param conversationId ID of the conversation
   * @param messageId ID of the message
   * @returns Metadata about the uploaded file
   */
  async uploadFile(
    fileData: string,
    fileName: string,
    mimeType: string,
    conversationId: string,
    messageId: string
  ): Promise<FileMetadata> {
    const result = await invoke('upload_file', {
      payload: {
        fileData,
        fileName,
        mimeType,
        conversationId,
        messageId
      }
    });
    
    return result as FileMetadata;
  }

  /**
   * Get a file from the Rust backend
   * @param filePath Path to the file
   * @param asBase64 Whether to return the file as base64
   * @returns File data
   */
  async getFile(filePath: string, asBase64: boolean = true): Promise<string> {
    return await invoke('get_file', {
      filePath,
      asBase64
    }) as string;
  }

  /**
   * Get a thumbnail for an image file
   * @param filePath Path to the image file
   * @returns Thumbnail data as base64
   */
  async getImageThumbnail(filePath: string): Promise<string> {
    return await invoke('get_image_thumbnail', {
      filePath
    }) as string;
  }

  /**
   * Optimize an image file
   * @param filePath Path to the image file
   * @param maxWidth Maximum width of the optimized image
   * @param maxHeight Maximum height of the optimized image
   * @param quality Quality of the optimized image (1-100)
   * @returns Optimized image data as base64
   */
  async optimizeImage(
    filePath: string,
    maxWidth: number = 1200,
    maxHeight: number = 1200,
    quality: number = 80
  ): Promise<string> {
    return await invoke('optimize_image', {
      filePath,
      options: {
        maxWidth,
        maxHeight,
        quality
      }
    }) as string;
  }

  /**
   * Extract metadata from a text file
   * @param filePath Path to the text file
   * @returns Metadata about the text file
   */
  async extractTextMetadata(filePath: string): Promise<any> {
    return await invoke('extract_text_metadata', {
      filePath
    });
  }

  /**
   * Extract code blocks from a text file
   * @param filePath Path to the text file
   * @returns Array of [language, code] tuples
   */
  async extractCodeBlocks(filePath: string): Promise<[string, string][]> {
    return await invoke('extract_code_blocks', {
      filePath
    }) as [string, string][];
  }

  /**
   * Extract metadata from an audio file
   * @param filePath Path to the audio file
   * @returns Metadata about the audio file
   */
  async extractAudioMetadata(filePath: string): Promise<any> {
    return await invoke('extract_audio_metadata', {
      filePath
    });
  }

  /**
   * Delete a file
   * @param filePath Path to the file
   * @returns Whether the file was deleted
   */
  async deleteFile(filePath: string): Promise<boolean> {
    return await invoke('delete_file', {
      filePath
    }) as boolean;
  }

  /**
   * Cleanup empty directories
   * @returns Whether the cleanup was successful
   */
  async cleanupEmptyDirectories(): Promise<boolean> {
    return await invoke('cleanup_empty_directories') as boolean;
  }
}

export const fileService = new FileService();
