import type { Tool, ToolResult } from './types';
import { fetch, ResponseType } from '@tauri-apps/api/http';

export type Image = {
  alt: string;
  url: string;
  context: string;
  description: string;
  preview: string;
  base64: string;
  name: string;
};

export type Link = {
  url: string;
  text: string;
  context: string;
  title: string;
};

export type Audio = {
  url: string;
  title: string;
  type: string;
  duration: string;
  context: string;
  base64: string;
};

export type Video = {
  url: string;
  title: string;
  thumbnail: string;
  duration: string;
  platform: 'youtube' | 'vimeo' | 'other';
  context: string;
};

export interface ExtractedContent {
  text: string;
  links: {
    urls: Link[];
    images: Image[];
    audio: Audio[];
    videos: Video[];
  };
}

export class WebFetcherTool implements Tool {
  name = 'webFetcher';
  description = 'Fetches and extracts text content and media links from web pages';
  parameters = {
    url: {
      type: 'string',
      description: 'The URL of the web page to fetch'
    }
  };

  toSchema() {
    return JSON.stringify({
      "name": this.name,
      "description": this.description,
      "parameters": this.parameters
    });
  }

  async execute(params: Record<string, any>): Promise<ToolResult> {
    const url = params.url;
    console.log(`WebFetcherTool: Starting fetch for URL: ${url}`);
    
    try {
      if (!url.match(/^https?:\/\/.+/)) {
        console.warn('WebFetcherTool: Invalid URL format:', url);
        return {
          success: false,
          result: 'Invalid URL format. URL must start with http:// or https://',
        };
      }

      console.log(`WebFetcherTool: Sending HTTP request to: ${url}`);
      const response = await fetch<string>(url, {
        method: 'GET',
        headers: {
          'User-Agent': 'Mozilla/5.0 (compatible; AIAssistant/1.0)',
        },
        responseType: ResponseType.Text,
      });

      if (!response.ok) {
        console.error(`WebFetcherTool: Failed to fetch URL:`, response.status);
        return {
          success: false,
          result: `Failed to fetch URL: ${response.status}`,
        };
      }

      console.log('WebFetcherTool: Successfully fetched URL, extracting content...');
      const html = response.data;
      const extracted = await this.extractContent(html, url);

      console.log('WebFetcherTool: Content extraction complete', {
        textLength: extracted.text.length,
        urls: extracted.links.urls.length,
        images: extracted.links.images.length,
        audio: extracted.links.audio.length,
        videos: extracted.links.videos.length
      });

      return {
        success: true,
        result: JSON.stringify(extracted, null, 2),
        metadata: {
          url,
          timestamp: new Date().toISOString(),
        }
      };

    } catch (error) {
      console.error('WebFetcherTool: Error during execution:', error);
      return {
        success: false,
        result: error instanceof Error ? error.message : 'Unknown error occurred',
      };
    }
  }

  private async extractContent(html: string, baseUrl: string): Promise<ExtractedContent> {
    console.log('WebFetcherTool: Starting content extraction');
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');
    
    console.log('WebFetcherTool: Cleaning document structure');
    ['script', 'style', 'head'].forEach(tag => {
      const removed = doc.querySelectorAll(tag).length;
      doc.querySelectorAll(tag).forEach(el => el.remove());
      console.log(`WebFetcherTool: Removed ${removed} ${tag} elements`);
    });

    // Convert HTML to Markdown-style text
    const convertToMarkdown = (element: Element): string => {
      let text = '';
      
      for (const node of Array.from(element.childNodes)) {
        if (node.nodeType === Node.TEXT_NODE) {
          text += node.textContent?.trim() + ' ';
        } else if (node instanceof Element) {
          const tag = node.tagName.toLowerCase();
          const content = convertToMarkdown(node).trim();
          
          switch (tag) {
            case 'h1': text += `\n# ${content}\n\n`; break;
            case 'h2': text += `\n## ${content}\n\n`; break;
            case 'h3': text += `\n### ${content}\n\n`; break;
            case 'h4': text += `\n#### ${content}\n\n`; break;
            case 'h5': text += `\n##### ${content}\n\n`; break;
            case 'h6': text += `\n###### ${content}\n\n`; break;
            case 'p': text += `\n${content}\n\n`; break;
            case 'br': text += '\n'; break;
            case 'strong':
            case 'b': text += `**${content}**`; break;
            case 'em':
            case 'i': text += `*${content}*`; break;
            case 'ul': text += `\n${content}`; break;
            case 'ol': text += `\n${content}`; break;
            case 'li': text += `- ${content}\n`; break;
            case 'blockquote': text += `\n> ${content}\n\n`; break;
            case 'img': 
              const src = node.getAttribute('src') || '';
              text += `${src}`; 
              break;
            case 'audio':
              const audioSrc = node.getAttribute('src') || '';
              text += `${audioSrc}`;
              break;
            default: text += content;
          }
        }
      }
      
      return text;
    };

    const text = convertToMarkdown(doc.body)
      .replace(/\n{3,}/g, '\n\n') // Replace multiple newlines with double newlines
      .trim();

    console.log(`WebFetcherTool: Extracted ${text.length} characters of text`);

    const links = {
      urls: [] as Link[],
      images: [] as Image[],
      audio: [] as Audio[],
      videos: [] as Video[]
    };

    // Regular links
    console.log('WebFetcherTool: Processing regular links');
    doc.querySelectorAll('a[href]').forEach(el => {
      const href = this.resolveUrl(el.getAttribute('href') || '', baseUrl);
      if (href) {
        links.urls.push({
          url: href,
          text: el.textContent?.trim() || '',
          context: this.getElementContext(el),
          title: el.getAttribute('title') || ''
        });
      }
    });

    // Image links
    console.log('WebFetcherTool: Processing images');
    await Promise.all(Array.from(doc.querySelectorAll('img[src]')).map(async el => {
      const src = this.resolveUrl(el.getAttribute('src') || '', baseUrl);
      if (src) {
        console.log(`WebFetcherTool: Fetching image: ${src}`);
        try {

          console.log(`WebFetcherTool: Successfully processed image: ${src}`);

          links.images.push({
            url: src,
            alt: el.getAttribute('alt') || '',
            context: this.getElementContext(el),
            description: el.getAttribute('title') || '',
            preview: '',
            base64: '',
            name: this.extractFilename(src)
          });
        } catch (error) {
          console.error(`WebFetcherTool: Failed to process image ${src}:`, error);
        }
      }
    }));

    // Audio links
    console.log('WebFetcherTool: Processing audio files');
    await Promise.all(Array.from(doc.querySelectorAll('audio[src], source[type^="audio"]')).map(async el => {
      const src = this.resolveUrl(el.getAttribute('src') || '', baseUrl);
      if (src) {
        console.log(`WebFetcherTool: Fetching audio: ${src}`);
        try {
          
          console.log(`WebFetcherTool: Successfully processed audio: ${src}`);
          
          links.audio.push({
            url: src,
            title: el.getAttribute('title') || this.extractFilename(src),
            type: el.getAttribute('type') || 'audio/unknown',
            duration: el.getAttribute('duration') || '',
            context: this.getElementContext(el),
            base64: ''
          });
        } catch (error) {
          console.error(`WebFetcherTool: Failed to process audio ${src}:`, error);
        }
      }
    }));

    // Video links
    console.log('WebFetcherTool: Processing videos');
    doc.querySelectorAll('video[src], source[type^="video"], iframe[src*="youtube"], iframe[src*="vimeo"]').forEach(el => {
      const src = this.resolveUrl(el.getAttribute('src') || '', baseUrl);
      if (src) {
        links.videos.push({
          url: src,
          title: el.getAttribute('title') || this.extractFilename(src

          ),
          thumbnail: el.getAttribute('poster') || '',
          duration: el.getAttribute('duration') || '',
          platform: this.getVideoPlatform(src),
          context: this.getElementContext(el)
        });
      }
    });

    console.log('WebFetcherTool: Content extraction complete');
    console.log('WebFetcherTool: Extracted content:', {
      textLength: text.length,
      urls: links.urls.length,
      images: links.images.length, 
      audio: links.audio.length,
      videos: links.videos.length
    });
    return {
      text,
      links: {
        urls: this.removeDuplicatesByUrl(links.urls),
        images: this.removeDuplicatesByUrl(links.images),
        audio: this.removeDuplicatesByUrl(links.audio),
        videos: this.removeDuplicatesByUrl(links.videos)
      }
    };
  }

  private getElementContext(el: Element, contextLength: number = 100): string {
    let context = '';
    let parent = el.parentElement;
    while (parent && !['article', 'section', 'div'].includes(parent.tagName.toLowerCase())) {
      parent = parent.parentElement;
    }
    if (parent) {
      context = parent.textContent?.replace(/\s+/g, ' ').trim() || '';
      if (context.length > contextLength) {
        context = context.substring(0, contextLength) + '...';
      }
    }
    return context;
  }

  private getVideoPlatform(url: string): 'youtube' | 'vimeo' | 'other' {
    if (url.includes('youtube.com') || url.includes('youtu.be')) return 'youtube';
    if (url.includes('vimeo.com')) return 'vimeo';
    return 'other';
  }

  private extractFilename(url: string): string {
    try {
      const pathname = new URL(url).pathname;
      return pathname.split('/').pop() || '';
    } catch {
      return '';
    }
  }

  private removeDuplicatesByUrl<T extends { url: string }>(items: T[]): T[] {
    const seen = new Set();
    return items.filter(item => {
      if (seen.has(item.url)) return false;
      seen.add(item.url);
      return true;
    });
  }

  private resolveUrl(url: string, base: string): string {
    try {
      return new URL(url, base).href;
    } catch {
      return '';
    }
  }
}

export const webFetcher = new WebFetcherTool(); 