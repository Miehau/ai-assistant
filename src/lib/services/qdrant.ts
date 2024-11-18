import { QdrantClient } from '@qdrant/js-client-rest';
import type { Message } from '$lib/types';
import type { 
  SearchRequest, 
  SearchResponse, 
  PointStruct,
  Vector
} from '@qdrant/js-client-rest';

export class QdrantService {
  private client: QdrantClient;

  constructor(url: string = 'http://localhost:6333', apiKey?: string) {
    this.client = new QdrantClient({ url, apiKey });
  }

  async search(
    collectionName: string, 
    vector: Vector,
    limit: number = 5,
    filter?: Record<string, any>
  ): Promise<SearchResponse> {
    try {
      const searchRequest: SearchRequest = {
        vector,
        limit,
        filter
      };

      return await this.client.search(collectionName, searchRequest);
    } catch (error) {
      console.error('Failed to search Qdrant:', error);
      throw error;
    }
  }

  async upsert(
    collectionName: string, 
    points: PointStruct[]
  ): Promise<void> {
    try {
      await this.client.upsert(collectionName, {
        points
      });
    } catch (error) {
      console.error('Failed to upsert points to Qdrant:', error);
      throw error;
    }
  }

  async createCollection(
    collectionName: string,
    vectorSize: number
  ): Promise<void> {
    try {
      await this.client.createCollection(collectionName, { 
        vectors: {
          size: vectorSize,
          distance: 'Cosine'
        }
      });
    } catch (error) {
      console.error('Failed to create collection:', error);
      throw error;
    }
  }

  async deleteCollection(collectionName: string): Promise<void> {
    try {
      await this.client.deleteCollection(collectionName);
    } catch (error) {
      console.error('Failed to delete collection:', error);
      throw error;
    }
  }
}

export const qdrantService = new QdrantService(); 